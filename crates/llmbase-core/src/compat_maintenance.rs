//! Legacy deterministic lint DTOs. Python may host explicitly replaced checks;
//! default scanning, checks and the check sequence are implemented here.
use crate::{Error, Result, compat_kb, compat_resolve, visibility};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashSet},
    io::Read,
    path::Path,
    sync::LazyLock,
};

static LINKS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]").unwrap());
const CHECKS: [&str; 8] = [
    "structural",
    "broken_links",
    "orphans",
    "missing_metadata",
    "dirty_tags",
    "duplicates",
    "stubs",
    "uncategorized",
];
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid(format!("{key} must be a string")))
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
        Value::Number(v) => v.as_f64() != Some(0.0),
    }
}
fn string(value: &Value) -> String {
    match value {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::String(v) => v.clone(),
        _ => value.to_string(),
    }
}
fn repr_string(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c)
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| Error::Invalid(format!("{key} must be an array")))
}
fn iterable(value: &Value) -> Result<Vec<Value>> {
    match value {
        Value::Array(v) => Ok(v.clone()),
        Value::String(v) => Ok(v.chars().map(|c| json!(c.to_string())).collect()),
        Value::Object(v) => Ok(v.keys().map(|s| json!(s)).collect()),
        _ => Err(Error::Invalid("tags must be iterable".into())),
    }
}
fn cjk(c: char) -> bool {
    matches!(c,'\u{3400}'..='\u{4dbf}'|'\u{4e00}'..='\u{9fff}')
}

pub fn load_articles(root: &Path, public_only: bool) -> Result<Value> {
    let paths = compat_kb::markdown_files(root, true)?;
    if paths.len() > 20_000 {
        return Err(Error::Invalid(
            "lint snapshot exceeds 20000 articles".into(),
        ));
    }
    let mut total = 0usize;
    let mut articles = Vec::new();
    for path in paths {
        let doc = compat_kb::read(&path)?;
        total += doc.original.len();
        if total > 64 * 1024 * 1024 {
            return Err(Error::Invalid("lint snapshot exceeds 64 MiB".into()));
        }
        if public_only && !visibility::is_public(&doc.metadata) {
            continue;
        }
        let slug = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Error::Invalid("article filename must be UTF-8".into()))?;
        let m = &doc.metadata;
        let tags = m
            .get("tags")
            .filter(|v| truthy(v))
            .cloned()
            .unwrap_or(json!([]));
        articles.push(json!({"path":path,"slug":slug,"title":m.get("title").cloned().unwrap_or(json!(slug)),"summary":m.get("summary").cloned().unwrap_or(json!("")),"tags":tags,"content":compat_kb::body(&doc),"metadata":m}));
    }
    Ok(json!(articles))
}

pub fn duplicate_candidates(articles: &[Value]) -> Result<Vec<(String, String)>> {
    if articles.len() > 2000 {
        return Err(Error::Invalid(
            "pairwise lint is bounded to 2000 articles".into(),
        ));
    }
    let mut pairs = Vec::new();
    let names = |a: &Value| -> Result<BTreeSet<String>> {
        let title = a["title"].as_str().unwrap_or(field(a, "slug")?);
        let mut names = BTreeSet::new();
        for text in title.split('/').chain(std::iter::once(field(a, "slug")?)) {
            let name: String = text.chars().filter(|c| cjk(*c)).collect();
            if !name.is_empty() {
                names.insert(compat_resolve::convert(&name, false)?);
                names.insert(name);
            }
        }
        Ok(names)
    };
    let prepared = articles
        .iter()
        .map(|a| {
            Ok((
                field(a, "slug")?.to_owned(),
                iterable(&a["tags"])?
                    .iter()
                    .map(|v| string(v).to_lowercase())
                    .collect::<BTreeSet<_>>(),
                names(a)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    for (index, (a, tags_a, names_a)) in prepared.iter().enumerate() {
        for (b, tags_b, names_b) in prepared.iter().skip(index + 1) {
            let mut score = 0;
            if a.chars().any(cjk) || b.chars().any(cjk) {
                if a.contains(b) || b.contains(a) {
                    score += 2
                }
            } else if (a.chars().count() >= 4 && b.contains(a))
                || (b.chars().count() >= 4 && a.contains(b))
            {
                score += 2
            }
            if !tags_a.is_empty()
                && !tags_b.is_empty()
                && tags_a.intersection(tags_b).count() as f64 / tags_a.union(tags_b).count() as f64
                    >= 0.6
            {
                score += 2
            }
            if names_a.iter().any(|a| {
                names_b.iter().any(|b| {
                    let (short, long) = if a.chars().count() <= b.chars().count() {
                        (a, b)
                    } else {
                        (b, a)
                    };
                    a == b
                        || (short.chars().count() > 1
                            && long.contains(short)
                            && short.chars().count() as f64 / long.chars().count() as f64 >= 0.67)
                })
            }) {
                score += 3
            }
            if score >= 2 {
                pairs.push((a.clone(), b.clone()))
            }
        }
    }
    Ok(pairs)
}

pub fn check(name: &str, args: &Value) -> Result<Value> {
    let articles = array(args, "articles")?;
    let mut issues = Vec::<String>::new();
    match name {
        "structural" => {
            if args["index_md"] != true {
                issues.push("Missing master index file (_index.md)".into())
            }
            if args["index_json"] != true {
                issues.push("Missing JSON index (index.json)".into())
            }
            if articles.is_empty() {
                issues.push("No articles in the wiki".into())
            }
        }
        "broken_links" => {
            let aliases = compat_resolve::pairs(&args["aliases"])?;
            let existing: HashSet<String> = if let Some(slugs) = args.get("existing_slugs") {
                serde_json::from_value(slugs.clone())?
            } else {
                articles
                    .iter()
                    .map(|a| field(a, "slug").map(str::to_lowercase))
                    .collect::<Result<_>>()?
            };
            for article in articles {
                for found in LINKS.captures_iter(field(article, "content")?) {
                    let target = found[1].trim();
                    let conversions = if args["cjk"] == false {
                        vec![]
                    } else {
                        vec![
                            compat_resolve::convert(target, false)?,
                            compat_resolve::convert(target, true)?,
                        ]
                    };
                    let resolved = compat_resolve::resolve(target, &aliases, &conversions);
                    if resolved
                        .as_ref()
                        .and_then(Value::as_str)
                        .is_some_and(|s| existing.contains(s))
                        || existing.contains(&target.to_lowercase().replace(' ', "-"))
                    {
                        continue;
                    }
                    issues.push(format!(
                        "Broken link in {}: [[{}]]",
                        field(article, "slug")?,
                        &found[1]
                    ));
                }
            }
        }
        "orphans" => {
            if args["backlinks"].is_null() {
                issues.push("Backlinks map not built yet".into())
            } else {
                let map = args["backlinks"]
                    .as_object()
                    .ok_or_else(|| Error::Invalid("backlinks must be an object".into()))?;
                for article in articles {
                    let slug = field(article, "slug")?;
                    if !map.contains_key(&slug.to_lowercase()) {
                        issues.push(format!("Orphan article (no incoming links): {slug}"))
                    }
                }
            }
        }
        "missing_metadata" => {
            for article in articles {
                for key in ["title", "summary", "tags"] {
                    if !truthy(&article["metadata"][key]) {
                        issues.push(format!("Missing {key}: {}", field(article, "slug")?))
                    }
                }
            }
        }
        "dirty_tags" => {
            for article in articles {
                let mut dirty = Vec::new();
                for tag in iterable(&article["tags"])? {
                    let item = if let Some(tag) = tag.as_str() {
                        if tag.chars().count() > 40 {
                            Some(format!("{}...", tag.chars().take(40).collect::<String>()))
                        } else if (tag.contains(' ') && tag.split_whitespace().count() > 4)
                            || [
                                "we need",
                                "the user",
                                "output",
                                "list ",
                                "tags:",
                                "tag1",
                                "tag2",
                                "interpret",
                                "based on",
                            ]
                            .iter()
                            .any(|s| tag.to_lowercase().contains(s))
                        {
                            Some(tag.to_owned())
                        } else {
                            None
                        }
                    } else {
                        Some(string(&tag))
                    };
                    if let Some(item) = item {
                        dirty.push(repr_string(&item))
                    }
                }
                if !dirty.is_empty() {
                    issues.push(format!(
                        "Dirty tags in {}: [{}]",
                        field(article, "slug")?,
                        dirty.join(", ")
                    ))
                }
            }
        }
        "stubs" => {
            for article in articles {
                let slug = field(article, "slug")?;
                let title = article["title"].as_str().unwrap_or("");
                let summary = article["summary"].as_str().unwrap_or("");
                let content = field(article, "content")?.trim();
                let issue = if title.contains("English Title / 中文标题")
                    || summary.contains("One-line summary in English")
                {
                    Some(format!("Unfilled template: {slug}"))
                } else if title.chars().all(|c| matches!(c, '.' | '/' | ' ')) {
                    Some(format!("Empty/garbage title: {slug}"))
                } else if [
                    "has not been fully written",
                    "has not been written yet",
                    "尚未完成撰写",
                ]
                .iter()
                .any(|s| content.contains(s))
                {
                    Some(format!("Placeholder stub: {slug}"))
                } else if [
                    "the user says",
                    "the user wants",
                    "the user asks",
                    "the user is",
                ]
                .iter()
                .any(|s| summary.to_lowercase().contains(s))
                {
                    Some(format!("LLM prompt leak: {slug}"))
                } else if args["allow_cjk_slugs"] != true
                    && !slug.is_empty()
                    && slug.chars().all(cjk)
                {
                    Some(format!("CJK slug (should be pinyin): {slug}"))
                } else if content.chars().count() < 50 && !truthy(&article["metadata"]["stub"]) {
                    Some(format!(
                        "Near-empty article: {slug} ({} chars)",
                        content.chars().count()
                    ))
                } else {
                    None
                };
                if let Some(issue) = issue {
                    issues.push(issue)
                }
            }
        }
        "duplicates" => {
            for (a, b) in duplicate_candidates(articles)? {
                issues.push(format!("Likely duplicate: {a} <-> {b}"))
            }
        }
        "uncategorized" => {
            for category in array(args, "categories")? {
                if category["id"] == "other" {
                    for article in category
                        .get("articles")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        issues.push(format!(
                            "Uncategorized article: {}",
                            field(article, "slug")?
                        ))
                    }
                }
            }
        }
        _ => return Err(Error::Capability("unknown native lint check".into())),
    }
    Ok(json!(issues))
}

pub fn transform(action: &str, args: &Value) -> Result<Value> {
    match action {
        "prepare" => {
            let root = Path::new(field(args, "concepts_dir")?);
            let meta = Path::new(field(args, "meta_dir")?);
            let articles = match args.get("articles") {
                Some(value) => value.clone(),
                None => load_articles(root, args["public_only"] == true)?,
            };
            let aliases = compat_resolve::read_pairs(&meta.join("aliases.json"))?;
            let backlinks = match std::fs::File::open(meta.join("backlinks.json")) {
                Ok(file) => {
                    let mut bytes = Vec::new();
                    file.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
                    if bytes.len() > 4 * 1024 * 1024 {
                        return Err(Error::Invalid("backlinks exceed 4 MiB".into()));
                    }
                    serde_json::from_slice::<Value>(&bytes)?
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Value::Null,
                Err(e) => return Err(e.into()),
            };
            Ok(
                json!({"articles":articles,"aliases":aliases,"backlinks":backlinks,"index_md":meta.join("_index.md").exists(),"index_json":meta.join("index.json").exists(),"allow_cjk_slugs":args["allow_cjk_slugs"]}),
            )
        }
        "load" => load_articles(
            Path::new(field(args, "concepts_dir")?),
            args["public_only"] == true,
        ),
        "duplicates" => Ok(json!(duplicate_candidates(array(args, "articles")?)?)),
        "check" => check(field(args, "name")?, args),
        _ => Err(Error::Capability("unknown local maintenance helper".into())),
    }
}

/// Explicit callback stages may replace checks; null means use the native check.
/// Empty lists remain actual results, never placeholders for an unimplemented op.
pub fn run_lint(
    mut args: Value,
    mut host: impl FnMut(&str, &Value) -> Result<Value>,
) -> Result<Value> {
    if args.get("articles").is_none() {
        args["articles"] = load_articles(
            Path::new(field(&args, "concepts_dir")?),
            args["public_only"] == true,
        )?;
    }
    let mut result = serde_json::Map::new();
    let mut total = 0;
    for name in CHECKS {
        let override_value = host(name, &args)?;
        let value = if override_value["override"] == true {
            override_value["value"].clone()
        } else {
            check(name, &args)?
        };
        total += value
            .as_array()
            .ok_or_else(|| Error::Invalid(format!("lint check {name} must return a list")))?
            .len();
        result.insert(name.into(), value);
    }
    result.insert("total_issues".into(), json!(total));
    let result = Value::Object(result);
    host(
        "emit",
        &json!({"event":"after_lint_check","total_issues":total,"results":result}),
    )?;
    Ok(result)
}
