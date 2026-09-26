//! Legacy query sequencing with explicit host callbacks for live prompts/models.
//! Retrieval, public-source checks, bounded context, filing and promotion are native.
use crate::{
    Error, Library, LibraryConfig, Result,
    compat_kb::{markdown_files, read},
    config::scoped_path,
    recovery::SourceGuard,
    store::WriterSession,
};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

static WORDS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{L}\p{N}_]+").unwrap());
static NON_WORDS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\p{L}\p{N}_]+").unwrap());
const LIMIT: usize = 16 * 1024 * 1024;
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid(format!("{key} must be a string")))
}
fn public(v: &Value) -> bool {
    // Share the fail-closed policy: malformed private/visibility flags deny publicity.
    crate::visibility::is_public(v)
}
fn take(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}
fn words(s: &str) -> HashSet<String> {
    WORDS
        .find_iter(&s.to_lowercase())
        .map(|m| m.as_str().to_owned())
        .collect()
}
fn string(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_owned()
}
fn full_text(doc: &crate::document::Document) -> String {
    doc.original.replace("\r\n", "\n").replace('\r', "\n")
}
fn bounded(value: &Value) -> Result<()> {
    if serde_json::to_vec(value)?.len() > LIMIT {
        Err(Error::Invalid("query data exceeds 16 MiB".into()))
    } else {
        Ok(())
    }
}

fn index_json(bytes: &[u8]) -> Result<Vec<Value>> {
    // Legacy index files can contain JSON escapes for isolated UTF-16 units.
    // Preserve valid pairs and escaped backslashes; match Python's '?' repair.
    let mut repaired = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let unit = |at: usize| -> Option<u16> {
        if bytes.get(at..at + 2)? != b"\\u" {
            return None;
        }
        u16::from_str_radix(std::str::from_utf8(bytes.get(at + 2..at + 6)?).ok()?, 16).ok()
    };
    while i < bytes.len() {
        if let Some(value) = unit(i) {
            if (0xd800..=0xdbff).contains(&value)
                && unit(i + 6).is_some_and(|next| (0xdc00..=0xdfff).contains(&next))
            {
                repaired.extend_from_slice(&bytes[i..i + 12]);
                i += 12;
                continue;
            }
            if (0xd800..=0xdfff).contains(&value) {
                repaired.push(b'?');
                i += 6;
                continue;
            }
        }
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            repaired.extend_from_slice(&bytes[i..i + 2]);
            i += 2;
        } else {
            repaired.push(bytes[i]);
            i += 1;
        }
    }
    Ok(serde_json::from_slice(&repaired)?)
}

/// Read the compatibility index, validating every row against its current source.
pub fn load_index(config: &LibraryConfig) -> Result<Value> {
    let path = scoped_path(&config.meta, "index.json")?;
    let rows: Vec<Value> = if path.exists() {
        if path.metadata()?.len() > LIMIT as u64 {
            return Err(Error::Invalid("query index exceeds 16 MiB".into()));
        }
        let bytes = std::fs::read(path)?;
        if bytes.len() > LIMIT {
            return Err(Error::Invalid("query index exceeds 16 MiB".into()));
        }
        index_json(&bytes)?
    } else {
        Vec::new()
    };
    let mut result = Vec::new();
    for mut row in rows {
        let Some(slug) = row.get("slug").and_then(Value::as_str) else {
            continue;
        };
        let Ok(path) = scoped_path(&config.concepts, &format!("{slug}.md")) else {
            continue;
        };
        if path.is_file() {
            let doc = read(&path)?;
            if public(&doc.metadata) {
                // The static index may retain a formerly private summary.
                for key in ["title", "summary", "tags"] {
                    row[key] = doc
                        .metadata
                        .get(key)
                        .cloned()
                        .unwrap_or_else(|| if key == "tags" { json!([]) } else { json!("") });
                }
                result.push(row);
            }
        }
    }
    Ok(json!(result))
}

/// The legacy overlap selection is kept separate from native FTS/hybrid ranking.
pub fn gather(config: &LibraryConfig, question: &str) -> Result<Value> {
    let query = words(question);
    let mut scored = Vec::new();
    let mut index_lines = Vec::new();
    for path in markdown_files(&config.concepts, false)? {
        let doc = read(&path)?;
        if !public(&doc.metadata) {
            continue;
        }
        let content = full_text(&doc);
        let title = string(&doc.metadata, "title");
        let summary = string(&doc.metadata, "summary");
        index_lines.push(format!("- {title}: {summary}"));
        let tags = doc
            .metadata
            .get("tags")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        let score = query
            .intersection(&words(&format!(
                "{title} {summary} {tags} {}",
                take(&content, 500)
            )))
            .count();
        if score > 0 {
            scored.push((score,json!({"path":path.file_name().unwrap().to_string_lossy(),"content":take(&content,4000)})));
        }
    }
    let mut contexts = Vec::new();
    // Reconstruct this projection from visible source metadata: old static files can
    // retain text from a source that has since become private or been deleted.
    if scoped_path(&config.meta, "_index.md")?.exists() && !index_lines.is_empty() {
        contexts.push(json!({"path":"_index.md","content":take(&index_lines.join("\n"),3000)}));
    }
    scored.sort_by_key(|a| std::cmp::Reverse(a.0));
    contexts.extend(scored.into_iter().take(15).map(|(_, v)| v));
    for path in markdown_files(&config.outputs, false)? {
        let doc = read(&path)?;
        if !public(&doc.metadata) {
            continue;
        }
        let content = full_text(&doc);
        if query.intersection(&words(&take(&content, 500))).count() > 1 {
            contexts.push(json!({"path":format!("outputs/{}",path.file_name().unwrap().to_string_lossy()),"content":take(&content,3000)}));
        }
    }
    let result = json!(contexts);
    bounded(&result)?;
    Ok(result)
}

/// Pure TF-IDF prefilter. A tokenizer callback supplies tokens, never ranked results.
pub fn transform(action: &str, args: &Value) -> Result<Value> {
    bounded(args)?;
    if action != "prefilter" {
        return Err(Error::Capability("unknown query transform".into()));
    }
    let index = args
        .get("index")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Invalid("index must be an array".into()))?;
    let top = args
        .get("top_k")
        .and_then(Value::as_i64)
        .unwrap_or(200)
        .max(0) as usize;
    let fallback = || json!(index.iter().take(top).cloned().collect::<Vec<_>>());
    if top == 0 {
        return Ok(json!([]));
    }
    let stop: HashSet<String> =
        serde_json::from_value(args.get("stopwords").cloned().unwrap_or(json!([])))?;
    let cjk: HashSet<String> =
        serde_json::from_value(args.get("cjk_stopwords").cloned().unwrap_or(json!([])))?;
    let tokenized = args.get("tokenized");
    let terms: Vec<String> = if let Some(t) = tokenized {
        serde_json::from_value(t["query"].clone())?
    } else {
        crate::compat_search::tokenize(text(args, "question")?, &stop, &cjk)
    };
    if terms.is_empty() {
        return Ok(fallback());
    }
    let supplied: Option<Vec<Vec<String>>> = tokenized
        .map(|t| serde_json::from_value(t["documents"].clone()))
        .transpose()?;
    if supplied
        .as_ref()
        .is_some_and(|tokens| tokens.len() != index.len())
    {
        return Err(Error::Invalid(
            "query tokenizer must cover every index row".into(),
        ));
    }
    let docs: Vec<(usize, Vec<String>)> = index
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let tags = row
                .get("tags")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|v| {
                            v.as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| v.to_string())
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let tokens = supplied.as_ref().map(|d| d[i].clone()).unwrap_or_else(|| {
                crate::compat_search::tokenize(
                    &format!("{} {} {tags}", string(row, "title"), string(row, "summary")),
                    &stop,
                    &cjk,
                )
            });
            (i, tokens)
        })
        .filter(|(_, t)| !t.is_empty())
        .collect();
    if docs.is_empty() {
        return Ok(fallback());
    }
    let idf: HashMap<_, _> = terms
        .iter()
        .map(|term| {
            let df = docs
                .iter()
                .filter(|(_, tokens)| tokens.contains(term))
                .count();
            (
                term.as_str(),
                ((docs.len() + 1) as f64 / (df + 1) as f64).ln() + 1.,
            )
        })
        .collect();
    let mut scores = Vec::new();
    for (i, tokens) in &docs {
        let mut counts = HashMap::new();
        for token in tokens {
            *counts.entry(token.as_str()).or_insert(0usize) += 1;
        }
        let mut score = 0.;
        for term in &terms {
            if let Some(count) = counts.get(term.as_str()) {
                score += (1. + (*count as f64).ln()) * idf[term.as_str()];
            }
        }
        if score > 0. {
            scores.push((*i, score));
        }
    }
    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    Ok(if scores.is_empty() {
        fallback()
    } else {
        json!(
            scores
                .into_iter()
                .take(top)
                .map(|(i, _)| index[i].clone())
                .collect::<Vec<_>>()
        )
    })
}

#[derive(Clone)]
struct Guard {
    id: String,
    revision: u64,
    hash: String,
}
fn guard_refs(guards: &[Guard]) -> Vec<SourceGuard<'_>> {
    guards
        .iter()
        .map(|g| SourceGuard {
            source_id: &g.id,
            revision: g.revision,
            content_hash: &g.hash,
        })
        .collect()
}
fn capture(config: &LibraryConfig) -> Result<Vec<Guard>> {
    let mut library = Library::open(config.clone())?;
    library.sync()?;
    Ok(library
        .list(None, false)?
        .into_iter()
        .map(|s| Guard {
            id: s.source_id,
            revision: s.revision,
            hash: s.content_hash,
        })
        .collect())
}
fn validate_snapshot(config: &LibraryConfig, expected: &[Guard]) -> Result<()> {
    let current = capture(config)?;
    if current.len() != expected.len()
        || current.iter().any(|actual| {
            !expected.iter().any(|old| {
                old.id == actual.id && old.revision == actual.revision && old.hash == actual.hash
            })
        })
    {
        return Err(Error::Conflict(
            "query source snapshot changed during retrieval".into(),
        ));
    }
    Ok(())
}
fn file_output(config: &LibraryConfig, args: &Value, guards: &[Guard]) -> Result<Value> {
    let question = text(args, "question")?;
    let answer = text(args, "answer")?;
    let format = args
        .get("output_format")
        .and_then(Value::as_str)
        .unwrap_or("markdown");
    let mut library = Library::open(config.clone())?;
    let (stamp, created): (String, String) = library.db.query_row(
        "SELECT strftime('%Y%m%d-%H%M%S','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let slug = take(&NON_WORDS.replace_all(&question.to_lowercase(), "-"), 50)
        .trim_matches('-')
        .to_owned();
    let mut filename = format!("{stamp}-{slug}.md");
    if scoped_path(&config.outputs, &filename)?.exists() {
        filename = format!("{stamp}-{slug}-{}.md", uuid::Uuid::new_v4().simple());
    }
    let metadata = json!({"title":question,"type":format!("query_{format}"),"created":created,"query_provenance":{"library_id":config.library_id,"sources":guards.iter().map(|g|json!({"source_id":g.id,"revision":g.revision,"content_hash":g.hash})).collect::<Vec<_>>()}});
    let bytes = format!(
        "---\n{}---\n\n{answer}",
        crate::document::yaml_metadata(&metadata)?
    );
    library.write_source_guarded_many(
        "note",
        &filename,
        bytes.into_bytes(),
        None,
        &guard_refs(guards),
    )?;
    let path = scoped_path(&config.outputs, &filename)?;
    Ok(json!(
        path.strip_prefix(&config.base)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or(filename)
    ))
}

fn promote(
    config: &LibraryConfig,
    args: &Value,
    guards: &[Guard],
    host: &mut impl FnMut(&str, &Value) -> Result<Value>,
) -> Result<Value> {
    let judged = host("judge", args)?;
    bounded(&judged)?;
    if let Some(error) = judged.get("error").and_then(Value::as_str) {
        return Ok(json!({"promoted":false,"reason":error}));
    }
    if judged.get("truncated") == Some(&json!(true)) {
        return Ok(json!({"promoted":false,"reason":"judge response was truncated"}));
    }
    let decision = judged.get("decision").cloned().unwrap_or(Value::Null);
    if !decision.is_object() {
        let kind = match decision {
            Value::Null => "NoneType",
            Value::Array(_) => "list",
            Value::Bool(_) => "bool",
            Value::String(_) => "str",
            Value::Number(_) => "int",
            _ => "dict",
        };
        return Ok(
            json!({"promoted":false,"reason":format!("judge returned non-object JSON: {kind}")}),
        );
    }
    if !decision.get("promote").is_some_and(|v| {
        v != &Value::Null
            && v != &json!(false)
            && v != &json!(0)
            && v != &json!("")
            && v != &json!([])
            && v != &json!({})
    }) {
        return Ok(
            json!({"promoted":false,"reason":decision.get("reason").cloned().unwrap_or(json!("judge declined"))}),
        );
    }
    let merge = decision
        .get("merge_into")
        .and_then(Value::as_str)
        .filter(|s| !matches!(s.trim().to_lowercase().as_str(), "" | "null" | "none"));
    let target = merge
        .map(str::trim)
        .or_else(|| decision.get("slug").and_then(Value::as_str))
        .unwrap_or("");
    let mut missing = Vec::new();
    if target.is_empty() {
        missing.push("slug")
    }
    for key in ["title", "content"] {
        if decision
            .get(key)
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            missing.push(key)
        }
    }
    if !missing.is_empty() {
        return Ok(
            json!({"promoted":false,"reason":format!("judge missing fields: {}",missing.join(", "))}),
        );
    }
    let slug = target
        .replace(['/', '\\'], "-")
        .replace("..", "")
        .trim_matches(['.', '-', '_', ' '])
        .to_owned();
    if slug.is_empty() {
        return Ok(json!({"promoted":false,"reason":"judge slug sanitized to empty"}));
    }
    let existed = scoped_path(&config.concepts, &format!("{slug}.md"))?.exists();
    let article = json!({"slug":slug,"title":decision["title"],"summary":decision.get("summary").cloned().unwrap_or(json!("")),"tags":decision.get("tags").cloned().unwrap_or(json!([])),"content":decision["content"],"sources":[{"plugin":"qa","url":"","title":args["question"],"question":args["question"],"created":judged.get("created").cloned().unwrap_or(Value::Null)}]});
    let result = crate::compat_compile::write_article_guarded(
        config,
        &json!({"article":article,"headers":judged["headers"]}),
        &guard_refs(guards),
        true,
    )?;
    if result["path"].is_null() {
        return Ok(json!({"promoted":false,"reason":"write_article rejected slug"}));
    }
    let mut library = Library::open(config.clone())?;
    library.rebuild_metadata_compat()?;
    Ok(
        json!({"promoted":true,"reason":decision.get("reason").cloned().unwrap_or(json!("")),"slug":result["slug"],"title":decision["title"],"path":result["path"],"merged":existed||result["slug"]!=slug}),
    )
}

/// Callback payloads: selector {question,index}, answer {question,context,deep,
/// output_format,tone}, judge {question,answer,consulted,index}. Credentials stay
/// with the host. Rust never serializes or persists a callback or model secret.
pub fn run(
    config: &LibraryConfig,
    args: &Value,
    mut host: impl FnMut(&str, &Value) -> Result<Value>,
) -> Result<Value> {
    bounded(args)?;
    let mode = args.get("mode").and_then(Value::as_str).unwrap_or("basic");
    let writes = mode == "file_output"
        || mode == "promote"
        || args.get("file_back") == Some(&json!(true))
        || (args.get("promote") == Some(&json!(true))
            && args.get("return_context") == Some(&json!(true)));
    let _session = if writes {
        Some(WriterSession::acquire(config)?)
    } else {
        None
    };
    let guards = if writes { capture(config)? } else { Vec::new() };
    let mut stages = Vec::new();
    let mut overrides = Vec::new();
    let mut callback = |stage: &str, payload: &Value| {
        stages.push(stage.to_owned());
        let result = host(stage, payload)?;
        bounded(&result)?;
        if matches!(
            stage,
            "index" | "gather" | "file_output" | "promotion_override"
        ) && !result.is_null()
        {
            overrides.push(stage.to_owned());
        }
        Ok(result)
    };
    let value = match mode {
        "load_index" => load_index(config)?,
        "gather" => gather(config, text(args, "question")?)?,
        "file_output" => file_output(config, args, &guards)?,
        "promote" => promote(config, args, &guards, &mut callback)?,
        "basic" | "deep" => {
            let question = text(args, "question")?;
            let deep = mode == "deep";
            let index = if deep {
                let custom = callback("index", &json!({}))?;
                if custom.is_null() {
                    load_index(config)?
                } else {
                    custom
                }
            } else {
                json!([])
            };
            if !index.is_array() {
                return Err(Error::Invalid(
                    "query index callback must return an array".into(),
                ));
            }
            if deep && index.as_array().unwrap().is_empty() {
                json!("Wiki is empty. Run `llmbase compile` first.")
            } else {
                let mut context = Vec::new();
                if deep {
                    let mut selector = index.clone();
                    let threshold = args
                        .get("prefilter_threshold")
                        .and_then(Value::as_i64)
                        .unwrap_or(500);
                    if (index.as_array().unwrap().len() as i64) > threshold {
                        let payload = json!({"question":question,"index":index,"top_k":args.get("prefilter_top_k").cloned().unwrap_or(json!(200))});
                        let ranked = callback("prefilter", &payload)?;
                        selector = if ranked.is_null() {
                            transform("prefilter", &payload)?
                        } else {
                            ranked
                        };
                    }
                    let selected =
                        callback("selector", &json!({"question":question,"index":selector}))?;
                    let selected = selected
                        .as_str()
                        .ok_or_else(|| Error::Invalid("selector callback must return text".into()))?
                        .to_lowercase();
                    for row in index.as_array().unwrap() {
                        if selected.contains(&string(row, "title").to_lowercase()) {
                            let path = scoped_path(
                                &config.concepts,
                                &format!("{}.md", text(row, "slug")?),
                            )?;
                            if !path.exists() {
                                continue;
                            }
                            let doc = read(&path)?;
                            if public(&doc.metadata) {
                                context
                                    .push(json!({"path":row["title"],"content":full_text(&doc)}));
                            }
                        }
                    }
                }
                if !deep || context.len() < 3 {
                    let overridden = callback("gather", &json!({"question":question}))?;
                    context = if overridden.is_null() {
                        gather(config, question)?
                    } else {
                        overridden
                    }
                    .as_array()
                    .cloned()
                    .ok_or_else(|| {
                        Error::Invalid("context callback must return an array".into())
                    })?;
                }
                bounded(&json!(context))?;
                if writes {
                    validate_snapshot(config, &guards)?;
                }
                if context.is_empty() {
                    let message = if deep {
                        "Could not find relevant articles for this question."
                    } else {
                        "No articles found in the wiki. Run `llmbase compile` first to build the wiki from raw documents."
                    };
                    if !deep && args.get("return_path") == Some(&json!(true)) {
                        json!({"answer":message,"output_path":null})
                    } else {
                        json!(message)
                    }
                } else {
                    let answer = callback(
                        "answer",
                        &json!({"question":question,"context":context,"deep":deep,"output_format":args.get("output_format").cloned().unwrap_or(json!("markdown")),"tone":args.get("tone").cloned().unwrap_or(json!("default"))}),
                    )?;
                    let answer = answer
                        .as_str()
                        .ok_or_else(|| Error::Invalid("answer callback must return text".into()))?;
                    let output = if args.get("file_back") == Some(&json!(true)) {
                        let payload = json!({"question":question,"answer":answer,"output_format":if deep{json!("markdown")}else{args.get("output_format").cloned().unwrap_or(json!("markdown"))}});
                        let custom = callback("file_output", &payload)?;
                        if custom.is_null() {
                            file_output(config, &payload, &guards)?
                        } else if custom.get("override") == Some(&json!(true)) {
                            custom["value"].clone()
                        } else {
                            custom
                        }
                    } else {
                        Value::Null
                    };
                    if deep && args.get("return_context") == Some(&json!(true)) {
                        let consulted: Vec<_> = index
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter(|row| context.iter().any(|c| c["path"] == row["title"]))
                            .map(|row| json!({"slug":row["slug"],"title":row["title"]}))
                            .collect();
                        let mut result =
                            json!({"answer":answer,"consulted":consulted,"output_path":output});
                        if args.get("promote") == Some(&json!(true)) {
                            let payload = json!({"question":question,"answer":answer,"consulted":consulted,"index":index});
                            let override_value = callback("promotion_override", &payload)?;
                            result["promotion"] = if override_value.is_null() {
                                match promote(config, &payload, &guards, &mut callback) {
                                    Ok(value) => value,
                                    Err(error) => {
                                        json!({"promoted":false,"reason":format!("error: {error}")})
                                    }
                                }
                            } else if override_value.get("override") == Some(&json!(true)) {
                                override_value["value"].clone()
                            } else {
                                override_value
                            };
                        }
                        result
                    } else if !deep && args.get("return_path") == Some(&json!(true)) {
                        json!({"answer":answer,"output_path":output})
                    } else {
                        json!(answer)
                    }
                }
            }
        }
        _ => return Err(Error::Invalid("unknown query mode".into())),
    };
    Ok(
        json!({"value":value,"trace":{"core":"rust","operation":"compat_query","orchestrator":"rust","model_boundary":"host_callback","stages":stages,"host_overrides":overrides,"default_retrieval_public_only":true,"writes":writes}}),
    )
}
