//! Native file contracts for the established Python KnowledgeBase facade.
//! These operations preserve the read-without-index side effects of that API.
use crate::{
    Error, LibraryConfig, Result,
    config::scoped_path,
    document::{Document, hash},
    store::WriterLock,
};
use ferrous_opencc::{OpenCC, config::BuiltinConfig};
use fs2::FileExt;
use regex::Regex;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::LazyLock,
};
use uuid::Uuid;

fn field<'a>(args: &'a Value, name: &str) -> Result<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid(format!("{name} must be a string")))
}
fn value(metadata: &Value, key: &str, default: Value) -> Value {
    metadata.get(key).cloned().unwrap_or(default)
}
pub(crate) fn read(path: &Path) -> Result<Document> {
    if path.metadata()?.len() > 64 * 1024 * 1024 {
        return Err(Error::Invalid("source exceeds 64 MiB".into()));
    }
    Document::parse(fs::read(path)?)
}
pub(crate) fn body(doc: &Document) -> String {
    // python-frontmatter.load uses text-mode universal newline decoding. Preserve
    // original bytes in Document while adapting the legacy content/scalar-offset DTO.
    doc.original[doc.body_start..]
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim_matches(python_whitespace)
        .to_owned()
}
fn python_whitespace(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}')
}

pub(crate) fn markdown_files(root: &Path, sorted: bool) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.path().extension().is_some_and(|ext| ext == "md") {
            if entry.file_type()?.is_symlink() {
                return Err(Error::Invalid(
                    "symlinks are not accepted as source paths".into(),
                ));
            }
            if entry.file_type()?.is_file() {
                files.push(entry.path());
            }
        }
    }
    if sorted {
        files.sort();
    }
    Ok(files)
}
fn stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
fn metadata(doc: &Document, slug: &str) -> Value {
    json!({"slug":slug,"title":value(&doc.metadata,"title",json!(slug)),
        "summary":value(&doc.metadata,"summary",json!("")),"tags":value(&doc.metadata,"tags",json!([]))})
}
fn tag_matches(tags: &Value, tag: &str) -> Result<bool> {
    match tags {
        Value::Array(values) => Ok(values.contains(&json!(tag))),
        Value::String(text) => Ok(text.contains(tag)),
        Value::Object(values) => Ok(values.contains_key(tag)),
        _ => Err(Error::Invalid("tags value is not iterable".into())),
    }
}
fn list(config: &LibraryConfig, args: &Value) -> Result<Value> {
    let mut result = Vec::new();
    for path in markdown_files(&config.concepts, true)? {
        let doc = read(&path)?;
        let row = metadata(&doc, &stem(&path));
        if let Some(tag) = args
            .get("tag")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            && !tag_matches(&row["tags"], tag)?
        {
            continue;
        }
        result.push(row);
    }
    Ok(json!({"articles":result}))
}

static FUZZY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\p{L}\p{N}_]").unwrap());
fn normalize(text: &str) -> String {
    text.trim_matches(python_whitespace).to_lowercase()
}
fn fuzzy(text: &str) -> String {
    let mut key = FUZZY.replace_all(&normalize(text), "").into_owned();
    for stop in ["the", "of", "in", "on", "and", "for", "its"] {
        key = key.replace(stop, "");
    }
    key
}

// serde_json's default map is sorted; legacy fuzzy resolution selects the first
// alias in the actual JSON file. Retain that insertion order without global map changes.
struct Aliases(Vec<(String, Value)>);
impl<'de> serde::Deserialize<'de> for Aliases {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Aliases;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an alias mapping")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut access: M,
            ) -> std::result::Result<Aliases, M::Error> {
                let mut aliases = Vec::new();
                while let Some(entry) = access.next_entry::<String, Value>()? {
                    aliases.push(entry);
                }
                Ok(Aliases(aliases))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}
pub(crate) fn resolve_alias(
    config: &LibraryConfig,
    slug: &str,
    cjk: bool,
) -> Result<Option<String>> {
    let path = config.meta.join("aliases.json");
    if !path.exists() || slug.is_empty() {
        return Ok(None);
    }
    let aliases: Aliases = serde_json::from_slice(&fs::read(path)?)?;
    let map = &aliases.0;
    let key = normalize(slug);
    let mut keys = vec![key.clone(), key.replace(' ', "-"), key.replace(' ', "")];
    if cjk {
        for conversion in [BuiltinConfig::T2s, BuiltinConfig::S2t] {
            let cc =
                OpenCC::from_config(conversion).map_err(|e| Error::Capability(e.to_string()))?;
            keys.push(normalize(&cc.convert(slug)));
        }
    }
    for key in keys {
        if let Some(value) = map
            .iter()
            .find(|(alias, _)| alias == &key)
            .and_then(|(_, v)| v.as_str())
        {
            return Ok(Some(value.into()));
        }
    }
    let target = fuzzy(slug);
    for (alias, value) in map {
        if fuzzy(alias) == target {
            return Ok(value.as_str().map(str::to_owned));
        }
    }
    Ok(None)
}
fn article_path(config: &LibraryConfig, slug: &str) -> Option<PathBuf> {
    scoped_path(&config.concepts, &format!("{slug}.md"))
        .ok()
        .filter(|p| p.exists())
}
fn get(config: &LibraryConfig, args: &Value, sections_only: bool) -> Result<Value> {
    let requested = field(args, "slug")?;
    let mut resolved = requested.to_owned();
    let mut path = article_path(config, &resolved);
    if path.is_none()
        && args.get("no_alias") != Some(&Value::Bool(true))
        && let Some(alias) = resolve_alias(
            config,
            requested,
            args.get("resolve_cjk") == Some(&Value::Bool(true)),
        )?
    {
        path = article_path(config, &alias);
        if path.is_some() {
            resolved = alias;
        }
    }
    let Some(path) = path else {
        return Ok(json!({"found":false,"slug":requested}));
    };
    let doc = read(&path)?;
    let content = body(&doc);
    if sections_only {
        return Ok(
            json!({"found":true,"slug":resolved,"title":value(&doc.metadata,"title",json!(resolved)),"sections":parse_sections(&content)}),
        );
    }
    let mut row = metadata(&doc, &resolved);
    row["found"] = json!(true);
    row["sources"] = value(&doc.metadata, "sources", json!([]));
    if let Some(section) = args
        .get("section")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        let sections = parse_sections(&content);
        let span = find_section(&sections, section);
        row["section"] = json!(section);
        row["section_found"] = json!(span.is_some());
        if let Some((start, end)) = span {
            row["content"] = json!(
                content
                    .chars()
                    .skip(start)
                    .take(end - start)
                    .collect::<String>()
            );
        }
    } else {
        row["content"] = json!(content);
    }
    Ok(row)
}

static HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ {0,3}(#{1,6})\s+(.+?)(?:\s+#+)?\s*$").unwrap());
static FENCE_OPEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^ {0,3}(`{3,}|~{3,})").unwrap());
static FENCE_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ {0,3}([`~]{3,})\s*$").unwrap());
static INVISIBLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\x00-\x1F\x{200B}-\x{200F}\x{202A}-\x{202E}\x{2060}-\x{206F}\x{3000}\x{FEFF}]+")
        .unwrap()
});
static PUNCT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[《》「」『』（）()【】\[\]：:，。、？?！!—–―…·・\-_/\\.\s]+").unwrap()
});
pub fn normalized_title(title: &str) -> String {
    PUNCT
        .replace_all(&INVISIBLE.replace_all(title, ""), "")
        .into_owned()
}
pub fn parse_sections(text: &str) -> Value {
    let mut nodes: Vec<Value> = Vec::new();
    let mut ancestors: Vec<(usize, String)> = Vec::new();
    let mut seen = HashMap::<String, usize>::new();
    let mut fence: Option<String> = None;
    let mut cursor = 0;
    // Python splitlines includes Unicode line separators; keep their scalar positions.
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((offset, ch)) = chars.next() {
        if matches!(
            ch,
            '\n' | '\r'
                | '\x0b'
                | '\x0c'
                | '\x1c'
                | '\x1d'
                | '\x1e'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        ) {
            let mut end = offset + ch.len_utf8();
            if ch == '\r' && chars.peek().is_some_and(|(_, c)| *c == '\n') {
                end = chars.next().unwrap().0 + 1;
            }
            lines.push(&text[start..end]);
            start = end;
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    for line in lines {
        if let Some(open) = &fence {
            if let Some(close) = FENCE_CLOSE.captures(line.trim_end_matches('\n'))
                && close[1].starts_with(open.chars().next().unwrap())
                && close[1].len() >= open.len()
            {
                fence = None;
            }
        } else if let Some(open) = FENCE_OPEN.captures(line) {
            fence = Some(open[1].into());
        } else if let Some(heading) = HEADING.captures(line.trim_end_matches('\n')) {
            let level = heading[1].len();
            let title = heading[2].trim_matches(python_whitespace).to_owned();
            while ancestors.last().is_some_and(|(l, _)| *l >= level) {
                ancestors.pop();
            }
            let mut titles: Vec<_> = ancestors.iter().map(|(_, t)| normalized_title(t)).collect();
            let norm = normalized_title(&title);
            titles.push(norm.clone());
            let short: String = norm.chars().take(20).collect();
            let digest = format!("{:x}", Sha1::digest(titles.join("›").as_bytes()));
            let base = format!(
                "h{level}-{}-{}",
                if short.is_empty() { "_" } else { &short },
                &digest[..6]
            );
            let count = seen.entry(base.clone()).or_default();
            *count += 1;
            let anchor = if *count == 1 {
                base
            } else {
                format!("{base}-{count}")
            };
            nodes.push(json!({"level":level,"title":title,"anchor":anchor,"start":cursor,"end":text.chars().count(),"children":[]}));
            ancestors.push((level, title));
        }
        cursor += line.chars().count();
    }
    for i in 0..nodes.len() {
        if let Some(end) = nodes[i + 1..]
            .iter()
            .find(|node| node["level"].as_u64() <= nodes[i]["level"].as_u64())
            .map(|node| node["start"].clone())
        {
            nodes[i]["end"] = end;
        }
    }
    fn nest(nodes: &[Value], cursor: &mut usize, parent: u64) -> Vec<Value> {
        let mut result = Vec::new();
        while *cursor < nodes.len() && nodes[*cursor]["level"].as_u64().unwrap() > parent {
            let mut node = nodes[*cursor].clone();
            *cursor += 1;
            node["children"] = json!(nest(nodes, cursor, node["level"].as_u64().unwrap()));
            result.push(node);
        }
        result
    }
    json!(nest(&nodes, &mut 0, 0))
}
pub fn find_section(sections: &Value, anchor: &str) -> Option<(usize, usize)> {
    for node in sections.as_array()? {
        if node["anchor"] == anchor {
            return Some((
                node["start"].as_u64()? as usize,
                node["end"].as_u64()? as usize,
            ));
        }
        if let Some(found) = find_section(&node["children"], anchor) {
            return Some(found);
        }
    }
    None
}

fn local_path(text: &str) -> bool {
    if text.starts_with("http://") || text.starts_with("https://") {
        return false;
    }
    let chars: Vec<_> = text.chars().take(3).collect();
    (text.starts_with('/') && text.matches('/').count() >= 2)
        || (chars.len() >= 3 && chars[1] == ':' && matches!(chars[2], '/' | '\\'))
        || text.starts_with("\\\\")
}
fn safe_meta(value: &Value) -> Value {
    match value {
        Value::String(text) if local_path(text) => Value::Null,
        Value::Array(items) => json!(
            items
                .iter()
                .map(safe_meta)
                .filter(|v| !v.is_null())
                .collect::<Vec<_>>()
        ),
        Value::Object(items) => Value::Object(
            items
                .iter()
                .map(|(k, v)| (k.clone(), safe_meta(v)))
                .filter(|(_, v)| !v.is_null())
                .collect(),
        ),
        _ => value.clone(),
    }
}
fn raw_list(config: &LibraryConfig) -> Result<Value> {
    if !config.raw.exists() {
        return Ok(json!([]));
    }
    let mut directories = Vec::new();
    for entry in fs::read_dir(&config.raw)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            directories.push(entry.path());
        }
    }
    directories.sort();
    let mut result = Vec::new();
    for directory in directories {
        let index = directory.join("index.md");
        let path = if index.exists() {
            scoped_path(
                &config.raw,
                &index.strip_prefix(&config.raw).unwrap().to_string_lossy(),
            )?
        } else {
            let Some(path) = markdown_files(&directory, false)?.into_iter().next() else {
                continue;
            };
            path
        };
        let doc = read(&path)?;
        let slug = directory.file_name().unwrap().to_string_lossy();
        let mut entry = json!({"path":slug,"title":value(&doc.metadata,"title",json!(slug)),"type":value(&doc.metadata,"type",json!("unknown")),"compiled":value(&doc.metadata,"compiled",json!(false)),"ingested_at":value(&doc.metadata,"ingested_at",json!(""))});
        for (key, value) in doc.metadata.as_object().unwrap() {
            if entry.get(key).is_none() {
                entry[key] = safe_meta(value);
            }
        }
        entry
            .as_object_mut()
            .unwrap()
            .retain(|key, value| key == "path" || !value.as_str().is_some_and(local_path));
        result.push(entry);
    }
    Ok(json!(result))
}
fn stats(config: &LibraryConfig) -> Result<Value> {
    let articles = markdown_files(&config.concepts, false)?;
    let outputs = markdown_files(&config.outputs, false)?;
    let raw_count = if config.raw.exists() {
        fs::read_dir(&config.raw)?
            .collect::<std::io::Result<Vec<_>>>()?
            .len()
    } else {
        0
    };
    let mut words = 0;
    for path in &articles {
        words += fs::read_to_string(path)?
            .split(python_whitespace)
            .filter(|s| !s.is_empty())
            .count();
    }
    Ok(
        json!({"articles":articles.len(),"raw_documents":raw_count,"filed_outputs":outputs.len(),"total_words":words}),
    )
}

/// Native, scoped atomic persistence; deliberately does not trigger a whole-library index.
pub fn raw_write(config: &LibraryConfig, args: &Value) -> Result<Value> {
    let relative = field(args, "path")?;
    let content: Vec<u8> = if let Some(bytes) = args.get("content_bytes") {
        serde_json::from_value(bytes.clone())
            .map_err(|_| Error::Invalid("content_bytes must contain byte values".into()))?
    } else {
        field(args, "content")?.as_bytes().to_vec()
    };
    let path = scoped_path(&config.raw, relative)?;
    if args.get("parse_markdown") != Some(&Value::Bool(false)) {
        Document::parse(content.clone())?;
    }
    fs::create_dir_all(&config.meta)?;
    let _lock = WriterLock::acquire(config)?;
    if let Some(expected) = args.get("expected_hash") {
        let current = match fs::read(&path) {
            Ok(bytes) => Some(hash(&bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if current.as_deref() != expected.as_str() {
            return Err(Error::Conflict(
                "raw source changed; no overwrite performed".into(),
            ));
        }
    }
    let parent = path
        .parent()
        .ok_or_else(|| Error::Invalid("source path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".llmbase-{}.stage", Uuid::new_v4()));
    let outcome = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&content)?;
        #[cfg(unix)]
        if let Some(mode) = args.get("permissions").and_then(Value::as_u64) {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode((mode & 0o7777) as u32))?;
        }
        file.sync_all()?;
        fs::rename(&temporary, &path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    outcome?;
    Ok(json!({"path":path,"content_hash":hash(&content)}))
}

fn visibility_snapshot(config: &LibraryConfig) -> Result<Value> {
    let mut result = json!({"compiled":{},"raw":{},"note":{},"has_nonpublic":false});
    let mut count = 0usize;
    let mut size = 0u64;
    for (kind, root) in [
        ("compiled", &config.concepts),
        ("raw", &config.raw),
        ("note", &config.outputs),
    ] {
        if !root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| entry.depth() == 0 || entry.file_name() != ".original")
        {
            let entry =
                entry.map_err(|_| Error::Invalid("cannot inspect visibility snapshot".into()))?;
            if entry.file_type().is_symlink() {
                return Err(Error::Invalid("symlink in visibility snapshot".into()));
            }
            if !entry.file_type().is_file() || entry.path().extension().is_none_or(|e| e != "md") {
                continue;
            }
            if !crate::store::is_source_path(
                entry.path().strip_prefix(root).map_err(|_| Error::Scope)?,
            ) {
                continue;
            }
            count += 1;
            size = size.saturating_add(
                entry
                    .metadata()
                    .map_err(|_| Error::Invalid("cannot inspect source metadata".into()))?
                    .len(),
            );
            if count > 20_000 || size > 64 * 1024 * 1024 {
                return Err(Error::Capability(
                    "visibility snapshot exceeds supported limits".into(),
                ));
            }
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| Error::Scope)?
                .to_string_lossy()
                .replace('\\', "/");
            let mut slug = relative.trim_end_matches(".md").to_owned();
            if kind == "raw" && slug.ends_with("/index") {
                slug.truncate(slug.len() - 6);
            }
            let doc = read(entry.path());
            // An unreadable or malformed document is never public by default.
            let public = doc
                .as_ref()
                .is_ok_and(|doc| crate::visibility::is_public(&doc.metadata));
            if !public {
                result["has_nonpublic"] = json!(true);
            }
            result[kind][slug] = json!({"public":public,"content_hash":doc.ok().map(|d|d.hash)});
        }
    }
    Ok(result)
}

fn trails(config: &LibraryConfig, args: &Value) -> Result<Value> {
    let path = scoped_path(&config.meta, "trails.json")?;
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(json!({"trails":[]}));
        }
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(Error::Capability("trails exceed 4 MiB".into()));
    }
    let data: Value = serde_json::from_slice(&bytes)?;
    if args.get("public_only") == Some(&json!(true)) {
        let rows = data
            .get("trails")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Invalid("trails must be an array".into()))?;
        if !rows.is_empty() {
            return Err(Error::Scope);
        }
        return Ok(json!({"trails":[]}));
    }
    Ok(data)
}

pub fn execute(config: &LibraryConfig, operation: &str, args: &Value) -> Result<Value> {
    let write = operation == "compat_kb_raw_write";
    let _read_lock = if write || crate::store::writer_session_active(config) {
        None
    } else {
        match File::open(config.meta.join("llmbase.writer.lock")) {
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
    let result = match operation {
        "compat_kb_get" => get(config, args, false)?,
        "compat_kb_sections" => get(config, args, true)?,
        "compat_kb_list" => list(config, args)?,
        "compat_kb_raw_list" => raw_list(config)?,
        "compat_kb_visibility" => visibility_snapshot(config)?,
        "compat_kb_trails" => trails(config, args)?,
        "compat_kb_raw_hash" => {
            let relative = field(args, "path")?;
            if !relative.ends_with(".original") {
                return Err(Error::Invalid(
                    "raw original hash requires .original".into(),
                ));
            }
            let path = scoped_path(&config.raw, relative)?;
            let value = match fs::metadata(&path) {
                Ok(meta) if meta.len() > 32 * 1024 * 1024 => {
                    return Err(Error::Invalid("raw original exceeds 32 MiB".into()));
                }
                Ok(_) => Some(hash(&fs::read(path)?)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            json!({"content_hash":value})
        }
        "compat_kb_stats" => stats(config)?,
        "compat_kb_health" => {
            let path = config.meta.join("health.json");
            if path.exists() {
                json!({"status":"ok","report":serde_json::from_slice::<Value>(&fs::read(path)?)?})
            } else {
                json!({"status":"ok","report":null,"message":"No health check has run yet"})
            }
        }
        "compat_kb_raw_write" => raw_write(config, args)?,
        _ => {
            return Err(Error::Capability(format!(
                "unknown compatibility operation: {operation}"
            )));
        }
    };
    Ok(
        json!({"result":result,"trace":{"core":"rust","operation":operation,"library_id":config.library_id,"storage":"native_filesystem","indexed":false,"writer_lock":write}}),
    )
}
