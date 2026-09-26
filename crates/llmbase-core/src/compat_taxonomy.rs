//! Trusted local taxonomy read/value helpers for the established Python facade.
//! These actions read only caller-selected local directories; they never open or
//! infer a library, write files, or select remote/HTTP paths.
use crate::{Error, Result, compat_kb, document::Document};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    io::Read,
    path::Path,
    sync::LazyLock,
};

const MAX_TREE_DEPTH: usize = 64;
const MAX_ARTICLES: usize = 20_000;
const MAX_CACHE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_READ_BYTES: u64 = 64 * 1024 * 1024;

static LOWERCASE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\p{Lowercase}$").unwrap());
static CASED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\p{Cased}$").unwrap());
static CASE_IGNORABLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\p{Case_Ignorable}$").unwrap());

fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid(format!("{key} must be a string")))
}

fn is(re: &LazyLock<Regex>, ch: char) -> bool {
    let mut buf = [0u8; 4];
    re.is_match(ch.encode_utf8(&mut buf))
}

fn push_mapped(out: &mut String, mapped: &[u32], original: char) {
    if mapped.iter().all(|&unit| unit == 0) {
        out.push(original);
        return;
    }
    for &unit in mapped {
        if unit == 0 {
            break;
        }
        match char::from_u32(unit) {
            Some(mapped) => out.push(mapped),
            None => return,
        }
    }
}

/// Python str.lower(): full mappings plus the contextual Greek final sigma.
fn python_lower(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (index, &ch) in chars.iter().enumerate() {
        if ch == '\u{3a3}' {
            let before = chars[..index]
                .iter()
                .rev()
                .find(|c| !is(&CASE_IGNORABLE, **c))
                .is_some_and(|c| is(&CASED, *c));
            let after = chars[index + 1..]
                .iter()
                .find(|c| !is(&CASE_IGNORABLE, **c))
                .is_some_and(|c| is(&CASED, *c));
            out.push(if before && !after {
                '\u{3c2}'
            } else {
                '\u{3c3}'
            });
        } else {
            push_mapped(&mut out, &unicode_case_mapping::to_lowercase(ch), ch);
        }
    }
    out
}

/// Python str.title(): titlecase the first cased letter of each uncased-separated
/// run with full (possibly multi-character) mappings, lowercase later cased ones.
fn python_title(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous_is_cased = false;
    for ch in text.chars() {
        let lower = is(&LOWERCASE, ch);
        let cased = is(&CASED, ch);
        if lower {
            if !previous_is_cased {
                push_mapped(&mut out, &unicode_case_mapping::to_titlecase(ch), ch);
            } else {
                out.push(ch);
            }
        } else if cased {
            if previous_is_cased {
                push_mapped(&mut out, &unicode_case_mapping::to_lowercase(ch), ch);
            } else {
                out.push(ch);
            }
        } else {
            out.push(ch);
        }
        previous_is_cased = cased;
    }
    out
}

fn slug(path: &Path) -> Result<&str> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::Invalid("article name must be UTF-8".into()))
}

fn read_bytes(path: &Path, consumed: &mut u64, limit: u64, limit_error: &str) -> Result<Vec<u8>> {
    let remaining = MAX_READ_BYTES.saturating_sub(*consumed);
    let cap = remaining.min(limit);
    let error = || {
        Error::Invalid(if remaining < limit {
            "taxonomy aggregate reads exceed 64 MiB".into()
        } else {
            limit_error.into()
        })
    };
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > cap {
        return Err(error());
    }
    let mut bytes = Vec::new();
    file.take(cap + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        return Err(error());
    }
    *consumed += bytes.len() as u64;
    Ok(bytes)
}

fn read_document(path: &Path, consumed: &mut u64) -> Result<Document> {
    // The same parser as compat_kb::read, with an action-wide allocation bound.
    Document::parse(read_bytes(
        path,
        consumed,
        MAX_READ_BYTES,
        "taxonomy document exceeds 64 MiB",
    )?)
}

fn read_articles(concepts_dir: &Path, consumed: &mut u64) -> Result<Vec<Value>> {
    let files = compat_kb::markdown_files(concepts_dir, true)?;
    if files.len() > MAX_ARTICLES {
        return Err(Error::Invalid(
            "taxonomy article load exceeds 20000 articles".into(),
        ));
    }
    let mut articles = Vec::with_capacity(files.len());
    for path in files {
        let doc = read_document(&path, consumed)?;
        let slug = slug(&path)?;
        // Defaults apply only to missing fields; explicit null and unknown
        // value types are preserved exactly as the legacy facade returned them.
        articles.push(json!({
            "slug": slug,
            "title": doc.metadata.get("title").cloned().unwrap_or(json!(slug)),
            "tags": doc.metadata.get("tags").cloned().unwrap_or(json!([])),
            "summary": doc.metadata.get("summary").cloned().unwrap_or(json!("")),
        }));
    }
    Ok(articles)
}

fn read_title_pairs(concepts_dir: &Path, consumed: &mut u64) -> Result<Value> {
    let files = compat_kb::markdown_files(concepts_dir, false)?;
    if files.len() > MAX_ARTICLES {
        return Err(Error::Invalid(
            "taxonomy title map exceeds 20000 articles".into(),
        ));
    }
    let mut pairs = Vec::with_capacity(files.len());
    for path in files {
        let doc = read_document(&path, consumed)?;
        let slug = slug(&path)?;
        let title = doc.metadata.get("title").cloned().unwrap_or(json!(slug));
        pairs.push(json!([slug, title]));
    }
    Ok(json!(pairs))
}

fn read_cache(path: &Path, consumed: &mut u64) -> Result<Value> {
    let bytes = read_bytes(
        path,
        consumed,
        MAX_CACHE_BYTES,
        "taxonomy cache exceeds 4 MiB",
    )?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn load(meta_dir: &Path) -> Result<Value> {
    let path = meta_dir.join("taxonomy.json");
    if !path.exists() {
        return Ok(json!({"categories": []}));
    }
    read_cache(&path, &mut 0)
}

fn articles_arg(args: &Value) -> Result<&[Value]> {
    args.get("articles")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Invalid("articles must be an array".into()))
        .map(|articles| articles.as_slice())
}

fn validate_articles(articles: &[Value]) -> Result<()> {
    if articles.len() > MAX_ARTICLES {
        return Err(Error::Invalid(
            "taxonomy fallback exceeds 20000 articles".into(),
        ));
    }
    for article in articles {
        let object = article
            .as_object()
            .ok_or_else(|| Error::Invalid("article must be an object".into()))?;
        if !object.get("slug").is_some_and(|s| s.is_string()) {
            return Err(Error::Invalid("article slug must be a string".into()));
        }
        match object.get("tags") {
            None => {}
            Some(Value::Array(tags)) => {
                if !tags.iter().all(|t| t.is_string()) {
                    return Err(Error::Invalid("article tags must be strings".into()));
                }
            }
            _ => return Err(Error::Invalid("article tags must be an array".into())),
        }
    }
    Ok(())
}

/// Exact _fallback_taxonomy: stable first-seen tie order, top-10 then count>=2,
/// duplicate slugs keep the last tag list for membership, each slug assigned once.
fn fallback(articles: &[Value]) -> Result<Value> {
    validate_articles(articles)?;
    if articles.is_empty() {
        return Ok(json!([]));
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut article_tags: HashMap<String, Vec<String>> = HashMap::new();
    for article in articles {
        let slug = article["slug"].as_str().unwrap();
        let tags: Vec<String> = article
            .get("tags")
            .and_then(Value::as_array)
            .map(|tags| {
                tags.iter()
                    .map(|t| python_lower(t.as_str().unwrap()).replace(' ', "-"))
                    .collect()
            })
            .unwrap_or_default();
        for tag in &tags {
            match counts.get_mut(tag.as_str()) {
                Some(count) => *count += 1,
                None => {
                    order.push(tag.clone());
                    counts.insert(tag.clone(), 1);
                }
            }
        }
        article_tags.insert(slug.to_owned(), tags);
    }
    let mut ranked: Vec<(String, usize)> = order
        .into_iter()
        .map(|tag| {
            let count = counts[tag.as_str()];
            (tag, count)
        })
        .collect();
    ranked.sort_by_key(|a| std::cmp::Reverse(a.1));
    let top: Vec<String> = ranked
        .into_iter()
        .take(10)
        .filter(|(_, count)| *count >= 2)
        .map(|(tag, _)| tag)
        .collect();
    if top.is_empty() {
        return Ok(json!([{
            "id": "all",
            "label": {"en": "All Articles", "zh": "全部文章", "ja": "全記事"},
            "children": [],
            "article_slugs": articles.iter().map(|a| a["slug"].clone()).collect::<Vec<_>>(),
        }]));
    }
    let mut assigned: HashSet<&str> = HashSet::new();
    let mut categories = Vec::new();
    for tag in top {
        let mut slugs = Vec::new();
        for article in articles {
            let slug = article["slug"].as_str().unwrap();
            if assigned.contains(slug) {
                continue;
            }
            if article_tags
                .get(slug)
                .is_some_and(|tags| tags.iter().any(|t| t == &tag))
            {
                slugs.push(json!(slug));
                assigned.insert(slug);
            }
        }
        if !slugs.is_empty() {
            categories.push(json!({
                "id": tag,
                "label": {"en": python_title(&tag.replace('-', " ")), "zh": tag, "ja": tag},
                "children": [],
                "article_slugs": slugs,
            }));
        }
    }
    let unassigned: Vec<Value> = articles
        .iter()
        .map(|a| a["slug"].as_str().unwrap())
        .filter(|slug| !assigned.contains(slug))
        .map(|slug| json!(slug))
        .collect();
    if !unassigned.is_empty() {
        categories.push(json!({
            "id": "other",
            "label": {"en": "Other", "zh": "其他", "ja": "その他"},
            "children": [],
            "article_slugs": unassigned,
        }));
    }
    Ok(json!(categories))
}

fn python_strip(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
}

fn has_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c, '\u{4e00}'..='\u{9fff}' | '\u{3400}'..='\u{4dbf}' | '\u{3040}'..='\u{309f}' | '\u{30a0}'..='\u{30ff}')
    })
}

fn localize_title_str(title: &str, lang: &str) -> Value {
    if title.is_empty() || !title.contains('/') || lang == "zh-en" {
        return json!(title);
    }
    let parts: Vec<&str> = title
        .split('/')
        .map(python_strip)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() < 2 {
        return json!(title);
    }
    if lang == "zh" || lang == "ja" {
        json!(
            parts
                .iter()
                .find(|part| has_cjk(part))
                .copied()
                .unwrap_or(parts[parts.len() - 1])
        )
    } else {
        json!(
            parts
                .iter()
                .find(|part| !has_cjk(part))
                .copied()
                .unwrap_or(parts[0])
        )
    }
}

/// Falsy values pass through unchanged, exactly like the legacy helper; truthy
/// non-strings are rejected instead of silently coerced.
fn localize_title_value(title: &Value, lang: &str) -> Result<Value> {
    let falsy = match title {
        Value::Null => true,
        Value::Bool(value) => !value,
        Value::Number(number) => number.as_f64() == Some(0.0),
        Value::String(text) => text.is_empty(),
        Value::Array(values) => values.is_empty(),
        Value::Object(entries) => entries.is_empty(),
    };
    if falsy {
        return Ok(title.clone());
    }
    match title.as_str() {
        Some(text) => Ok(localize_title_str(text, lang)),
        None => Err(Error::Invalid("title must be a string".into())),
    }
}

fn tree_nodes(args: &Value) -> Result<&[Value]> {
    args.get("tree")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| Error::Invalid("tree must be an array".into()))
}

/// Iterative shape/depth validation before any recursive processing, so a deep
/// or malformed tree fails with a clear error instead of panicking.
fn validate_tree(nodes: &[Value]) -> Result<()> {
    let mut articles = 0usize;
    let mut stack: Vec<(&Value, usize)> = nodes.iter().map(|node| (node, 1)).collect();
    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_TREE_DEPTH {
            return Err(Error::Invalid("taxonomy tree exceeds depth 64".into()));
        }
        let object = node
            .as_object()
            .ok_or_else(|| Error::Invalid("taxonomy node must be an object".into()))?;
        if !object.contains_key("id") {
            return Err(Error::Invalid("taxonomy node requires an id".into()));
        }
        match object.get("label") {
            None | Some(Value::String(_)) | Some(Value::Object(_)) => {}
            _ => {
                return Err(Error::Invalid(
                    "taxonomy label must be a string or object".into(),
                ));
            }
        }
        match object.get("article_slugs") {
            None => {}
            Some(Value::Array(slugs)) => {
                articles += slugs.len();
                if articles > MAX_ARTICLES {
                    return Err(Error::Invalid(
                        "taxonomy tree exceeds 20000 articles".into(),
                    ));
                }
                if !slugs.iter().all(|s| s.is_string()) {
                    return Err(Error::Invalid(
                        "taxonomy article_slugs must be strings".into(),
                    ));
                }
            }
            _ => {
                return Err(Error::Invalid(
                    "taxonomy article_slugs must be an array".into(),
                ));
            }
        }
        match object.get("children") {
            None => {}
            Some(Value::Array(children)) => {
                stack.extend(children.iter().map(|child| (child, depth + 1)));
            }
            _ => return Err(Error::Invalid("taxonomy children must be an array".into())),
        }
    }
    Ok(())
}

/// Child-first global slug dedup, exactly like the legacy build orchestration.
fn dedup_nodes(nodes: &mut [Value], seen: &mut HashSet<String>) {
    for node in nodes {
        if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
            dedup_nodes(children, seen);
        }
        if let Some(slugs) = node.get_mut("article_slugs").and_then(Value::as_array_mut) {
            let mut unique = Vec::with_capacity(slugs.len());
            for slug in slugs.iter() {
                let text = slug.as_str().unwrap();
                if seen.insert(text.to_owned()) {
                    unique.push(json!(text));
                }
            }
            *slugs = unique;
        }
    }
}

fn display_label(node: &Value, lang: &str) -> Value {
    match node.get("label") {
        Some(Value::String(label)) => json!(label),
        Some(Value::Object(label)) => [lang, "en", "zh"]
            .iter()
            .find_map(|key| label.get(*key))
            .cloned()
            .unwrap_or_else(|| node["id"].clone()),
        // A missing label behaves like the legacy default {}; invalid shapes
        // were already rejected by validate_tree.
        _ => node["id"].clone(),
    }
}

fn localize_nodes(nodes: &[Value], lang: &str, titles: &Value) -> Result<Vec<Value>> {
    let mut result = Vec::with_capacity(nodes.len());
    for node in nodes {
        let empty = Vec::new();
        let slugs = node
            .get("article_slugs")
            .and_then(Value::as_array)
            .unwrap_or(&empty);
        let mut articles = Vec::with_capacity(slugs.len());
        for slug in slugs {
            let slug = slug.as_str().unwrap();
            let title = titles.get(slug).cloned().unwrap_or(json!(slug));
            articles.push(json!({"slug": slug, "title": localize_title_value(&title, lang)?}));
        }
        let children = match node.get("children").and_then(Value::as_array) {
            Some(children) => localize_nodes(children, lang, titles)?,
            None => Vec::new(),
        };
        let child_count: u64 = children.iter().map(|c| c["total"].as_u64().unwrap()).sum();
        let count = articles.len() as u64;
        result.push(json!({
            "id": node["id"].clone(),
            "label": display_label(node, lang),
            "count": count,
            "total": count + child_count,
            "articles": articles,
            "children": children,
        }));
    }
    Ok(result)
}

fn localize_tree(args: &Value) -> Result<Value> {
    let nodes = tree_nodes(args)?;
    let lang = text(args, "lang")?;
    let titles = args
        .get("titles")
        .ok_or_else(|| Error::Invalid("titles is required".into()))?;
    if !titles.is_object() {
        return Err(Error::Invalid("titles must be an object".into()));
    }
    validate_tree(nodes)?;
    Ok(json!(localize_nodes(nodes, lang, titles)?))
}

fn build(args: &Value) -> Result<Value> {
    let concepts_dir = Path::new(text(args, "concepts_dir")?);
    let meta_dir = Path::new(text(args, "meta_dir")?);
    let lang = text(args, "lang")?;
    let cache = meta_dir.join("taxonomy.json");
    let mut consumed = 0;
    let mut titles = None;
    let mut raw_tree: Vec<Value> = if cache.exists() {
        let cached = read_cache(&cache, &mut consumed)?;
        let object = cached
            .as_object()
            .ok_or_else(|| Error::Invalid("taxonomy cache must be an object".into()))?;
        match object.get("categories") {
            None => Vec::new(),
            Some(Value::Array(categories)) => categories.clone(),
            _ => {
                return Err(Error::Invalid(
                    "taxonomy categories must be an array".into(),
                ));
            }
        }
    } else {
        let articles = read_articles(concepts_dir, &mut consumed)?;
        let tree = fallback(&articles)?;
        titles = Some(
            articles
                .iter()
                .map(|article| {
                    (
                        article["slug"].as_str().unwrap().to_owned(),
                        article["title"].clone(),
                    )
                })
                .collect::<serde_json::Map<String, Value>>(),
        );
        tree.as_array().map(|a| a.to_vec()).unwrap_or_default()
    };
    validate_tree(&raw_tree)?;
    let mut seen = HashSet::new();
    dedup_nodes(&mut raw_tree, &mut seen);
    let titles = match titles {
        Some(titles) => titles,
        None => {
            let pairs = read_title_pairs(concepts_dir, &mut consumed)?;
            pairs
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| (pair[0].as_str().unwrap().to_owned(), pair[1].clone()))
                .collect()
        }
    };
    Ok(json!(localize_nodes(&raw_tree, lang, &json!(titles))?))
}

/// Filesystem actions are exposed only through the trusted local Python ABI,
/// never generic HTTP operation dispatch or a caller-selected remote path.
pub fn transform(action: &str, args: &Value) -> Result<Value> {
    match action {
        "load_articles" => Ok(json!(read_articles(
            Path::new(text(args, "concepts_dir")?),
            &mut 0
        )?)),
        "title_map" => read_title_pairs(Path::new(text(args, "concepts_dir")?), &mut 0),
        "load" => load(Path::new(text(args, "meta_dir")?)),
        "fallback" => fallback(articles_arg(args)?),
        "localize_title" => {
            let title = args
                .get("title")
                .ok_or_else(|| Error::Invalid("title is required".into()))?;
            localize_title_value(title, text(args, "lang")?)
        }
        "localize_tree" => localize_tree(args),
        "build" => build(args),
        _ => Err(Error::Capability("unknown local taxonomy helper".into())),
    }
}
