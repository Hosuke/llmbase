//! Legacy Python search contract. Storage, snapshots, tokenization and ranking stay native.
//! Python-only tokenizers supply tokens for an explicitly validated source snapshot.
use crate::{
    Error, LibraryConfig, Result,
    document::{Document, hash},
};
use fs2::FileExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static WORDS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{L}\p{N}_]+").unwrap());

fn cjk_like(ch: char) -> bool {
    matches!(ch as u32,
        0x3040..=0x309f | 0x30a0..=0x30ff | 0x31f0..=0x31ff | 0xff65..=0xff9f |
        0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff |
        0x20000..=0x3ffff)
}

/// Python's legacy tokenizer: lowercased L/N/_ words, then original CJK scalars and bigrams.
/// Stopwords are request data; no override is cached when the extension loads.
pub fn tokenize(
    text: &str,
    stopwords: &HashSet<String>,
    cjk_stopwords: &HashSet<String>,
) -> Vec<String> {
    let lowered = text.to_lowercase();
    let words: String = lowered
        .chars()
        .map(|ch| if cjk_like(ch) { ' ' } else { ch })
        .collect();
    let mut tokens: Vec<String> = WORDS
        .find_iter(&words)
        .map(|m| m.as_str())
        .filter(|word| word.chars().count() > 1 && !stopwords.contains(*word))
        .map(str::to_owned)
        .collect();
    let mut run = Vec::new();
    let flush = |run: &mut Vec<char>, tokens: &mut Vec<String>| {
        tokens.extend(
            run.iter()
                .map(char::to_string)
                .filter(|ch| !cjk_stopwords.contains(ch)),
        );
        tokens.extend(run.windows(2).map(|pair| pair.iter().collect::<String>()));
        run.clear();
    };
    for ch in text.chars() {
        if cjk_like(ch) {
            run.push(ch);
        } else {
            flush(&mut run, &mut tokens);
        }
    }
    flush(&mut run, &mut tokens);
    tokens
}

#[derive(Debug, Clone, Serialize)]
pub struct CompatDocument {
    pub source_id: String,
    pub content_hash: String,
    pub text: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub tags: Value,
    pub source: String,
    pub source_url: String,
    pub rel_path: String,
    pub path: String,
}

fn validate_kind(kind: &str) -> Result<()> {
    if !matches!(kind, "raw" | "compiled") {
        return Err(Error::Invalid(
            "compat search kind must be raw or compiled".into(),
        ));
    }
    Ok(())
}

fn string_field(metadata: &Value, name: &str, fallback: &str) -> Result<String> {
    match metadata.get(name) {
        None => Ok(fallback.into()),
        Some(Value::String(value)) => Ok(value.clone()),
        Some(_) => Err(Error::Invalid(format!(
            "legacy search {name} must be a string"
        ))),
    }
}

fn convert(
    config: &LibraryConfig,
    document: Document,
    root: &Path,
    path: &Path,
    kind: &str,
) -> Result<CompatDocument> {
    let relative = path.strip_prefix(root).map_err(|_| Error::Scope)?;
    let rel_path = relative
        .to_str()
        .ok_or_else(|| Error::Invalid("non-UTF-8 source path".into()))?
        .to_owned();
    let stem = relative.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    // python-frontmatter strips outer whitespace from its body representation.
    let decoded_body = document
        .original
        .get(document.body_start..)
        .ok_or_else(|| Error::Conflict("stored body offset is invalid".into()))?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let body = decoded_body.trim();
    let mut title = if kind == "raw" && document.metadata.get("title") == Some(&Value::Null) {
        stem.into()
    } else {
        string_field(&document.metadata, "title", stem)?
    };
    if kind == "raw" && title.is_empty() {
        title = stem.into();
    }
    // Raw search never reads compiled-only summary/tags metadata.
    let summary = if kind == "raw" {
        String::new()
    } else {
        string_field(&document.metadata, "summary", "")?
    };
    let tags = if kind == "raw" {
        json!([])
    } else {
        document
            .metadata
            .get("tags")
            .cloned()
            .unwrap_or_else(|| json!([]))
    };
    let source_url = document
        .metadata
        .get("source")
        .and_then(Value::as_str)
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
        .unwrap_or("")
        .to_owned();
    let text = if kind == "raw" {
        format!("{title} {title} {source_url} {body}")
    } else {
        // The legacy implementation joins a string's individual characters too.
        let joined = match &tags {
            Value::Array(values) => values
                .iter()
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| Error::Invalid("legacy tags must contain strings".into()))
                })
                .collect::<Result<Vec<_>>>()?
                .join(" "),
            Value::String(value) => value
                .chars()
                .map(|ch| ch.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            _ => {
                return Err(Error::Invalid(
                    "legacy tags must be an iterable of strings".into(),
                ));
            }
        };
        format!("{title} {title} {summary} {joined} {body}")
    };
    let source_name = if relative.components().count() > 1 {
        relative
            .components()
            .next()
            .and_then(|c| c.as_os_str().to_str())
            .unwrap_or(stem)
    } else {
        stem
    }
    .to_owned();
    let source_id = hash(&serde_json::to_vec(&(
        &config.library_id,
        root,
        kind,
        &rel_path,
    ))?);
    Ok(CompatDocument {
        source_id,
        content_hash: document.hash,
        text,
        slug: stem.into(),
        title,
        summary,
        tags,
        source: source_name,
        source_url,
        rel_path,
        path: path.to_string_lossy().into_owned(),
    })
}

fn scan_paths(
    root: &Path,
    recursive: bool,
    public_only: bool,
    result: &mut Vec<PathBuf>,
) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let mut directories = Vec::new();
    // Retain the filesystem's iteration order, as pathlib.glob/rglob do for tied legacy scores.
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if public_only && entry.file_name() == ".original" {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(Error::Invalid("source tree contains a symlink".into()));
        }
        if kind.is_dir() {
            directories.push(entry.path());
        }
        if kind.is_file() && entry.path().extension().is_some_and(|s| s == "md") {
            result.push(entry.path());
        }
    }
    if recursive {
        for directory in directories {
            scan_paths(&directory, true, public_only, result)?;
        }
    }
    Ok(())
}

fn scan_documents(
    config: &LibraryConfig,
    kind: &str,
    private: bool,
) -> Result<Vec<CompatDocument>> {
    validate_kind(kind)?;
    if kind == "compiled" && !config.concepts.exists() {
        return Ok(Vec::new());
    }
    let roots = if kind == "raw" {
        vec![&config.raw]
    } else {
        vec![&config.concepts, &config.outputs]
    };
    let mut documents = Vec::new();
    for root in roots {
        let mut paths = Vec::new();
        scan_paths(root, kind == "raw", !private, &mut paths)?;
        for path in paths {
            if !private
                && !crate::store::is_source_path(path.strip_prefix(root).map_err(|_| Error::Scope)?)
            {
                continue;
            }
            let parsed = (|| -> Result<Document> {
                if path.metadata()?.len() > 64 * 1024 * 1024 {
                    return Err(Error::Invalid("source exceeds 64 MiB".into()));
                }
                Document::parse(std::fs::read(&path)?)
            })();
            let document = match parsed {
                Ok(document) => document,
                Err(_) if kind == "raw" => continue, // Legacy search_raw skips unreadable/malformed files.
                Err(error) => return Err(error),
            };
            if !private && !crate::visibility::is_public(&document.metadata) {
                continue;
            }
            documents.push(convert(config, document, root, &path, kind)?);
        }
    }
    Ok(documents)
}

fn snapshot_documents(
    config: &LibraryConfig,
    kind: &str,
    private: bool,
) -> Result<Vec<CompatDocument>> {
    // Native writers already have a shared lock path. Read-only legacy libraries must not create
    // a database, directory, or lock merely to search. File descriptors release locks on drop.
    let session = crate::store::writer_session_active(config);
    let lock = if session {
        None
    } else {
        match std::fs::File::open(config.meta.join("llmbase.writer.lock")) {
            Ok(file) => {
                FileExt::try_lock_shared(&file).map_err(|error| {
                    if error.kind() == std::io::ErrorKind::WouldBlock {
                        Error::Busy
                    } else {
                        error.into()
                    }
                })?;
                Some(file)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        }
    };
    let first = scan_documents(config, kind, private)?;
    if lock.is_none() && !session {
        let second = scan_documents(config, kind, private)?;
        if snapshot(&config.library_id, kind, &first)?
            != snapshot(&config.library_id, kind, &second)?
        {
            return Err(Error::Conflict(
                "source snapshot changed while reading; retry search".into(),
            ));
        }
    }
    Ok(first)
}

fn snapshot(library_id: &str, kind: &str, docs: &[CompatDocument]) -> Result<String> {
    let ids: Vec<_> = docs
        .iter()
        .map(|doc| (&doc.source_id, &doc.content_hash))
        .collect();
    Ok(hash(&serde_json::to_vec(&(library_id, kind, ids))?))
}

fn trace(
    config: &LibraryConfig,
    snapshot: &str,
    documents: usize,
    callback: bool,
    private: bool,
) -> Value {
    json!({"core":"rust", "operation":"compat_search", "algorithm":"legacy-tfidf-v1",
        "library_id":config.library_id, "snapshot":snapshot, "documents":documents,
        "storage":"native_filesystem_snapshot", "indexed":false,
        "visibility":if private {"all"} else {"public"},
        "tokenizer":if callback {"python_callback"} else {"rust_legacy"}})
}

pub fn documents(config: &LibraryConfig, kind: &str, private: bool) -> Result<Value> {
    let documents = snapshot_documents(config, kind, private)?;
    let snapshot = snapshot(&config.library_id, kind, &documents)?;
    Ok(json!({"snapshot":snapshot, "documents":documents,
        "trace":{"core":"rust","operation":"compat_search_documents",
                 "library_id":config.library_id,"snapshot":snapshot,"documents":documents.len(),
                 "visibility":if private {"all"} else {"public"}}}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CallbackDocument {
    source_id: String,
    content_hash: String,
    tokens: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CallbackTokens {
    snapshot: String,
    query_terms: Vec<String>,
    documents: Vec<CallbackDocument>,
}

fn string_set(args: &Value, key: &str) -> Result<HashSet<String>> {
    serde_json::from_value(args.get(key).cloned().unwrap_or_else(|| json!([])))
        .map_err(|_| Error::Invalid(format!("{key} must be an array of strings")))
}

/// Exact legacy snippet window uses Python scalar offsets, including lower-case expansion.
fn snippet(text: &str, terms: &[String]) -> String {
    let lowered = text.to_lowercase();
    let length = text.chars().count();
    let best = terms
        .iter()
        .filter_map(|term| {
            lowered
                .find(term)
                .map(|offset| lowered[..offset].chars().count())
        })
        .min()
        .unwrap_or(length);
    if best >= length {
        return text.chars().take(200).collect();
    }
    text.chars()
        .skip(best.saturating_sub(100))
        .take((best + 100).min(length) - best.saturating_sub(100))
        .collect::<String>()
        .replace('\n', " ")
        .trim()
        .into()
}

pub fn search(config: &LibraryConfig, args: &Value, private: bool) -> Result<Value> {
    let private = private && args.get("public_only") != Some(&Value::Bool(true));
    let kind = args
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("compiled");
    validate_kind(kind)?;
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid("query must be a string".into()))?;
    let top_k = match args.get("top_k") {
        None => Some(10_i64),
        Some(Value::Null) if kind == "compiled" => None,
        Some(value) => Some(
            value
                .as_i64()
                .ok_or_else(|| Error::Invalid("top_k must be an integer".into()))?,
        ),
    };
    let callback = args.get("tokenized").is_some();
    let stopwords = string_set(args, "stopwords")?;
    let cjk_stopwords = string_set(args, "cjk_stopwords")?;
    let query_terms = if callback {
        Vec::new()
    } else {
        tokenize(query, &stopwords, &cjk_stopwords)
    };
    let empty_callback = args.get("tokenized").is_some_and(|value| {
        value
            .get("query_terms")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
    });
    // Legacy empty queries and raw zero limits do not parse any documents. In particular,
    // a malformed compiled file must not turn an empty query into a new error.
    if (kind == "raw" && top_k.is_some_and(|limit| limit <= 0))
        || (!callback && query_terms.is_empty())
        || empty_callback
    {
        let mut receipt = trace(config, "", 0, callback, private);
        receipt["snapshot"] = Value::Null;
        receipt["short_circuit"] = json!(true);
        return Ok(json!({"results":[],"trace":receipt}));
    }
    let docs = snapshot_documents(config, kind, private)?;
    let snapshot = snapshot(&config.library_id, kind, &docs)?;
    let (terms, tokens) = if let Some(value) = args.get("tokenized") {
        let mut supplied: CallbackTokens = serde_json::from_value(value.clone())?;
        if supplied.snapshot != snapshot {
            return Err(Error::Conflict(
                "tokenizer snapshot changed; retry search".into(),
            ));
        }
        let mut by_id = HashMap::new();
        for supplied_doc in supplied.documents.drain(..) {
            if by_id
                .insert(supplied_doc.source_id.clone(), supplied_doc)
                .is_some()
            {
                return Err(Error::Invalid("duplicate tokenizer source".into()));
            }
        }
        if by_id.len() != docs.len() {
            return Err(Error::Invalid(
                "tokenizer must cover the complete source snapshot".into(),
            ));
        }
        let mut tokens = Vec::new();
        for doc in &docs {
            let supplied = by_id.remove(&doc.source_id).ok_or(Error::Scope)?;
            if supplied.content_hash != doc.content_hash {
                return Err(Error::Conflict("tokenizer source hash mismatch".into()));
            }
            tokens.push(supplied.tokens);
        }
        (supplied.query_terms, tokens)
    } else {
        (
            query_terms,
            docs.iter()
                .map(|doc| tokenize(&doc.text, &stopwords, &cjk_stopwords))
                .collect(),
        )
    };
    // Raw search omits tokenless documents from its corpus, unlike compiled search.
    let corpus: Vec<_> = docs
        .iter()
        .zip(tokens)
        .filter(|(_, tokens)| kind != "raw" || !tokens.is_empty())
        .collect();
    let idf: HashMap<&str, f64> = terms
        .iter()
        .map(|term| {
            let count = corpus
                .iter()
                .filter(|(_, tokens)| tokens.contains(term))
                .count();
            (
                term.as_str(),
                ((corpus.len() + 1) as f64 / (count + 1) as f64).ln() + 1.0,
            )
        })
        .collect();
    let mut results = Vec::new();
    for (doc, tokens) in &corpus {
        let mut counts = HashMap::<&str, usize>::new();
        for token in tokens {
            *counts.entry(token.as_str()).or_default() += 1;
        }
        let mut score = 0.0;
        let mut matched = Vec::new();
        for term in &terms {
            if let Some(count) = counts.get(term.as_str()) {
                score += (1.0 + (*count as f64).ln()) * idf[term.as_str()];
                matched.push(term.clone());
            }
        }
        if score <= 0.0 {
            continue;
        }
        let rounded = (score * 1000.0).round_ties_even() / 1000.0;
        let snippet = snippet(&doc.text, &terms);
        let result = if kind == "raw" {
            json!({"source":doc.source,"source_url":doc.source_url,"title":doc.title,
                "rel_path":doc.rel_path,"score":rounded,"matched_terms":matched,"snippet":snippet})
        } else {
            json!({"slug":doc.slug,"title":doc.title,"summary":doc.summary,"score":rounded,
                "matched_terms":matched,"snippet":snippet,"path":doc.path})
        };
        results.push((rounded, result));
    }
    // Stable sort preserves the source order for ties just as Python list.sort does.
    results.sort_by(|a, b| b.0.total_cmp(&a.0));
    let count = match top_k {
        None => results.len(),
        Some(limit) if limit < 0 && kind == "compiled" => {
            results.len().saturating_sub(limit.unsigned_abs() as usize)
        }
        Some(limit) => usize::try_from(limit.max(0))
            .unwrap_or(usize::MAX)
            .min(results.len()),
    };
    let results: Vec<Value> = results
        .into_iter()
        .take(count)
        .map(|(_, result)| result)
        .collect();
    Ok(json!({"results":results,"trace":trace(config,&snapshot,corpus.len(),callback,private)}))
}
