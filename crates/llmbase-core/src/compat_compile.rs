//! Compatibility DTOs around the shared native document/profile/write machinery.
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use regex::Regex;
use serde_json::{Map, Value, json};

use crate::{
    Error, Library, LibraryConfig, Result,
    compat_kb::{body, markdown_files, read},
    config::scoped_path,
    document::hash,
    profile::{LanguageProfile, LanguageSection},
    store::WriterSession,
};

static SLUG_PUNCTUATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\s\x{1c}-\x{1f}/\\?#&:]+").unwrap());
static HYPHENS: LazyLock<Regex> = LazyLock::new(|| Regex::new("-+").unwrap());

fn field<'a>(args: &'a Value, name: &str) -> Result<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid(format!("{name} must be a string")))
}
fn trim(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
}

pub fn sanitize_slug(slug: &str) -> String {
    let slug = SLUG_PUNCTUATION.replace_all(slug, "-").replace("..", "");
    HYPHENS
        .replace_all(&slug, "-")
        .trim_matches(['.', '-', '_', ' '])
        .to_owned()
}

fn profile(args: &Value) -> Result<LanguageProfile> {
    let headers: Vec<(String, String)> = serde_json::from_value(
        args.get("headers")
            .cloned()
            .ok_or_else(|| Error::Invalid("headers are required".into()))?,
    )?;
    if headers.len() > 128
        || headers
            .iter()
            .any(|(key, header)| key.len() > 1024 || header.len() > 4096)
    {
        return Err(Error::Invalid(
            "section headers exceed compatibility limits".into(),
        ));
    }
    Ok(LanguageProfile {
        sections: headers
            .into_iter()
            .enumerate()
            .map(|(i, (key, header))| LanguageSection {
                code: format!("compat{i}"),
                key,
                header,
                extra: Map::new(),
            })
            .collect(),
        default_lang: "compat0".into(),
        api_default_lang: "compat0".into(),
        views: Vec::new(),
    })
}

fn article_block(block: &str) -> Option<Value> {
    let lines: Vec<_> = trim(block).split('\n').collect();
    let mut meta = Map::new();
    let mut content_start = 0;
    for (i, line) in lines.iter().enumerate() {
        if trim(line) == "---" {
            content_start = i + 1;
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            match trim(key) {
                key @ ("slug" | "title" | "summary") => {
                    meta.insert(key.into(), json!(trim(value)));
                }
                "tags" => {
                    meta.insert(
                        "tags".into(),
                        json!(trim(value).split(',').map(trim).collect::<Vec<_>>()),
                    );
                }
                _ => {}
            }
        }
    }
    if !meta.contains_key("slug") {
        return None;
    }
    meta.insert(
        "content".into(),
        json!(trim(&lines[content_start..].join("\n"))),
    );
    Some(Value::Object(meta))
}

fn update_block(block: &str) -> Option<Value> {
    let mut slug = None;
    let mut content = Vec::new();
    let mut in_append = false;
    for line in trim(block).split('\n') {
        if trim(line).starts_with("slug:") {
            slug = line.split_once(':').map(|(_, value)| trim(value));
        } else if trim(line).starts_with("append:") {
            in_append = true;
            let rest = trim(line.split_once(':').unwrap().1);
            if !rest.is_empty() && rest != "|" {
                content.push(rest);
            }
        } else if in_append {
            content.push(line);
        }
    }
    let slug = slug.filter(|s| !s.is_empty())?;
    Some(json!({"slug":slug,"content":trim(&content.join("\n"))}))
}

fn parse_response(response: &str) -> Result<Value> {
    let mut articles = Vec::new();
    for (marker, kind, parse) in [
        (
            "===ARTICLE===",
            "new",
            article_block as fn(&str) -> Option<Value>,
        ),
        (
            "===UPDATE===",
            "update",
            update_block as fn(&str) -> Option<Value>,
        ),
    ] {
        for part in response.split(marker).skip(1) {
            let block = part
                .split_once("===END===")
                .map(|(block, _)| block)
                .unwrap_or(part);
            if let Some(mut article) = parse(trim(block)) {
                article["type"] = json!(kind);
                articles.push(article);
                if articles.len() > 1000 {
                    return Err(Error::Invalid(
                        "compile response has too many articles".into(),
                    ));
                }
            }
        }
    }
    Ok(json!(articles))
}

/// Pure default helpers: no configuration lookup, filesystem access, or library creation.
pub fn transform(action: &str, args: &Value) -> Result<Value> {
    if serde_json::to_vec(args)?.len() > 16 * 1024 * 1024 {
        return Err(Error::Invalid(
            "compile transform input exceeds 16 MiB".into(),
        ));
    }
    match action {
        "sanitize_slug" => Ok(json!(sanitize_slug(field(args, "slug")?))),
        "parse_response" => parse_response(field(args, "response")?),
        "parse_article" => Ok(article_block(field(args, "block")?).unwrap_or(Value::Null)),
        "parse_update" => Ok(update_block(field(args, "block")?).unwrap_or(Value::Null)),
        "split" => Ok(json!(profile(args)?.split(field(args, "content")?))),
        "assemble" => {
            let sections: HashMap<String, String> = serde_json::from_value(
                args.get("sections")
                    .cloned()
                    .ok_or_else(|| Error::Invalid("sections are required".into()))?,
            )?;
            Ok(json!(profile(args)?.assemble(&sections)))
        }
        _ => Err(Error::Capability("unknown native compile transform".into())),
    }
}

fn checked(config: &LibraryConfig, kind: &str, relative: &str) -> Result<PathBuf> {
    scoped_path(config.root_for(kind)?, relative)
}

fn list_existing(config: &LibraryConfig) -> Result<Value> {
    markdown_files(&config.concepts, false)?
        .into_iter()
        .map(|path| {
            let doc = read(&path)?;
            let slug = path.file_stem().unwrap().to_string_lossy();
            let title = doc
                .metadata
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(&slug);
            Ok(json!(format!("{slug} ({title})")))
        })
        .collect::<Result<Vec<_>>>()
        .map(Value::Array)
}

fn find_uncompiled(config: &LibraryConfig) -> Result<Value> {
    if !config.raw.exists() {
        return Ok(json!([]));
    }
    let mut dirs = fs::read_dir(&config.raw)?.collect::<std::io::Result<Vec<_>>>()?;
    dirs.sort_by_key(|entry| entry.path());
    let mut uncompiled = Vec::new();
    for entry in dirs {
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let relative = entry.file_name().to_string_lossy().into_owned();
        let index = checked(config, "raw", &format!("{relative}/index.md"))?;
        if index.exists() {
            if !truthy(read(&index)?.metadata.get("compiled")) {
                uncompiled.push(index);
            }
        } else {
            for path in markdown_files(&entry.path(), true)? {
                if !truthy(read(&path)?.metadata.get("compiled")) {
                    uncompiled.push(path);
                    break;
                }
            }
        }
    }
    Ok(json!(uncompiled))
}

pub(crate) fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(v)) => *v,
        Some(Value::Number(v)) => v.as_f64() != Some(0.0),
        Some(Value::String(v)) => !v.is_empty(),
        Some(Value::Array(v)) => !v.is_empty(),
        Some(Value::Object(v)) => !v.is_empty(),
    }
}

fn json_file(path: &Path, default: Value) -> Result<Value> {
    if !path.exists() {
        return Ok(default);
    }
    if path.metadata()?.len() > 64 * 1024 * 1024 {
        return Err(Error::Invalid("metadata file exceeds 64 MiB".into()));
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn read_raw(config: &LibraryConfig, args: &Value) -> Result<Value> {
    let path = checked(config, "raw", field(args, "relative")?)?;
    let doc = read(&path)?;
    let mut content = body(&doc);
    if content.trim().is_empty() && args.get("attachments").and_then(Value::as_bool) == Some(true) {
        for entry in fs::read_dir(path.parent().unwrap())? {
            let entry = entry?;
            if entry.file_type()?.is_symlink() || !entry.file_type()?.is_file() {
                continue;
            }
            let file = entry.path();
            if file
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| matches!(s, "txt" | "py" | "json" | "csv"))
            {
                if entry.metadata()?.len() > 64 * 1024 * 1024 {
                    return Err(Error::Invalid("attachment exceeds 64 MiB".into()));
                }
                let bytes = fs::read(&file)?;
                let mut remaining = bytes.as_slice();
                let mut decoded = String::new();
                while !remaining.is_empty() {
                    match std::str::from_utf8(remaining) {
                        Ok(text) => {
                            decoded.push_str(text);
                            break;
                        }
                        Err(error) => {
                            decoded.push_str(
                                std::str::from_utf8(&remaining[..error.valid_up_to()]).unwrap(),
                            );
                            let offset = error.valid_up_to()
                                + error
                                    .error_len()
                                    .unwrap_or(remaining.len() - error.valid_up_to());
                            remaining = &remaining[offset..];
                        }
                    }
                }
                let decoded = decoded.replace("\r\n", "\n").replace('\r', "\n");
                content.push_str(&format!(
                    "\n\n## File: {}\n\n```\n{}\n```",
                    entry.file_name().to_string_lossy(),
                    decoded.chars().take(5000).collect::<String>()
                ));
            }
        }
    }
    Ok(
        json!({"metadata":doc.metadata,"content":content,"content_hash":hash(doc.original.as_bytes())}),
    )
}

fn now(library: &Library) -> Result<String> {
    Ok(library
        .db
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
            r.get(0)
        })?)
}
fn rendered(metadata: &Value, content: &str) -> Result<Vec<u8>> {
    Ok(format!(
        "---\n{}---\n{content}",
        crate::document::yaml_metadata(metadata)?
    )
    .into_bytes())
}
fn source_key(value: &Value) -> Vec<Value> {
    ["plugin", "url", "work_id", "title"]
        .iter()
        .map(|key| value.get(key).cloned().unwrap_or(json!("")))
        .collect()
}
fn cjk(value: &str) -> String {
    value
        .chars()
        .filter(|c| matches!(*c,'\u{3400}'..='\u{4dbf}'|'\u{4e00}'..='\u{9fff}'))
        .collect()
}

fn write_article(config: &LibraryConfig, args: &Value) -> Result<Value> {
    write_article_guarded(config, args, &[], false)
}

pub(crate) fn write_article_guarded(
    config: &LibraryConfig,
    args: &Value,
    guards: &[crate::recovery::SourceGuard<'_>],
    public_only: bool,
) -> Result<Value> {
    let article = args
        .get("article")
        .filter(|a| a.is_object())
        .ok_or_else(|| Error::Invalid("article must be a mapping".into()))?;
    let mut library = Library::open(config.clone())?;
    let _session = WriterSession::acquire(config)?;
    library.sync()?;
    let profile = profile(args)?;
    let requested_merge = args.get("relative").and_then(Value::as_str);
    let slug = if let Some(relative) = requested_merge {
        checked(config, "compiled", relative)?;
        Path::new(relative)
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Error::Invalid("merge filename missing".into()))?
            .to_owned()
    } else {
        sanitize_slug(field(article, "slug")?)
    };
    if slug.is_empty() {
        return Ok(json!({"path":null,"slug":slug}));
    }
    crate::compile::validate_slug(&slug)?;
    let title = article
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(&slug);
    let exact = requested_merge
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{slug}.md"));
    let mut target = checked(config, "compiled", &exact)?
        .exists()
        .then_some(exact);
    if target.is_none() && requested_merge.is_some() {
        return Err(Error::NotFound("merge target".into()));
    }
    if target.is_none() {
        for candidate in std::iter::once(slug.as_str())
            .chain(std::iter::once(title))
            .chain(title.split('/').map(trim).filter(|s| !s.is_empty()))
        {
            if let Some(resolved) = library.maintenance_resolve_article(candidate, true)?
                && resolved != slug
            {
                target = Some(format!("{resolved}.md"));
                break;
            }
        }
    }
    if target.is_none() {
        let incoming = cjk(title);
        if !incoming.is_empty() {
            for source in library.list(Some("compiled"), true)? {
                let previous = cjk(&source.title);
                if previous.is_empty() {
                    continue;
                }
                let (short, long) = if incoming.chars().count() <= previous.chars().count() {
                    (&incoming, &previous)
                } else {
                    (&previous, &incoming)
                };
                if incoming == previous
                    || (short.chars().count() >= 2
                        && long.contains(short)
                        && short.chars().count() * 5 >= long.chars().count() * 3)
                {
                    target = Some(source.rel_path);
                    break;
                }
            }
        }
    }
    let mut expected = None;
    let new_content = article.get("content").and_then(Value::as_str).unwrap_or("");
    let (mut metadata, content, mut changed) = if let Some(relative) = &target {
        let original = read(&checked(config, "compiled", relative)?)?;
        if public_only && !crate::visibility::is_public(&original.metadata) {
            return Err(Error::Scope);
        }
        expected = Some(hash(original.original.as_bytes()));
        if trim(new_content).is_empty() {
            return Ok(
                json!({"path":checked(config,"compiled",relative)?,"slug":slug,"changed":false}),
            );
        }
        let (merged, changed) = profile.merge_with_status(&body(&original), new_content);
        (original.metadata, merged, changed)
    } else {
        (
            json!({"title":title,"summary":article.get("summary").cloned().unwrap_or(json!("")),"tags":article.get("tags").cloned().unwrap_or(json!([])),"sources":article.get("sources").cloned().unwrap_or(json!([])),"created":now(&library)?}),
            new_content.to_owned(),
            true,
        )
    };
    if target.is_some() {
        if let Some(new_sources) = article
            .get("sources")
            .and_then(Value::as_array)
            .filter(|s| !s.is_empty())
        {
            let mut sources = metadata
                .get("sources")
                .cloned()
                .unwrap_or(json!([]))
                .as_array()
                .cloned()
                .ok_or_else(|| Error::Invalid("sources must be an array".into()))?;
            for source in new_sources {
                if !sources
                    .iter()
                    .any(|old| source_key(old) == source_key(source))
                {
                    sources.push(source.clone());
                    changed = true;
                }
            }
            metadata["sources"] = json!(sources);
        }
        if changed {
            let mut tags = BTreeSet::new();
            for values in [metadata.get("tags"), article.get("tags")]
                .into_iter()
                .flatten()
            {
                for tag in values
                    .as_array()
                    .ok_or_else(|| Error::Invalid("tags must be an array".into()))?
                {
                    tags.insert(
                        tag.as_str()
                            .ok_or_else(|| Error::Invalid("tags must be strings".into()))?
                            .to_owned(),
                    );
                }
            }
            metadata["tags"] = json!(tags);
        }
    }
    let relative = target.unwrap_or_else(|| format!("{slug}.md"));
    let mut raw_guard = None;
    if let Some(source) = args.get("raw_source").filter(|source| !source.is_null()) {
        let raw_relative = field(source, "relative")?;
        let wanted = field(source, "content_hash")?;
        let raw = library
            .list(Some("raw"), true)?
            .into_iter()
            .find(|s| s.rel_path == raw_relative)
            .ok_or_else(|| Error::NotFound("compile raw source".into()))?;
        if raw.content_hash != wanted {
            return Err(Error::Conflict("compile raw source changed".into()));
        }
        // Classify both sides with the shared fail-closed policy and retain the
        // strongest visibility; inheritance must never downgrade restrictions.
        let incoming = crate::visibility::visibility(&raw.metadata);
        let old = crate::visibility::visibility(&metadata);
        let strongest = if incoming == "private" || old == "private" {
            "private"
        } else if incoming == "internal" || old == "internal" {
            "internal"
        } else {
            "public"
        };
        if strongest != old {
            metadata["visibility"] = json!(strongest);
            changed = true;
        }
        raw_guard = Some(raw);
    }
    if changed {
        metadata["updated"] = json!(now(&library)?);
        let bytes = rendered(&metadata, &content)?;
        let mut all_guards: Vec<_> = guards
            .iter()
            .map(|g| crate::recovery::SourceGuard {
                source_id: g.source_id,
                revision: g.revision,
                content_hash: g.content_hash,
            })
            .collect();
        if let Some(raw) = &raw_guard {
            all_guards.push(crate::recovery::SourceGuard {
                source_id: &raw.source_id,
                revision: raw.revision,
                content_hash: &raw.content_hash,
            });
        }
        if !all_guards.is_empty() {
            library.write_source_guarded_many(
                "compiled",
                &relative,
                bytes,
                expected.as_deref(),
                &all_guards,
            )?;
        } else {
            library.write_source("compiled", &relative, bytes, expected.as_deref())?;
        }
    }
    let final_slug = Path::new(&relative)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&slug);
    Ok(json!({"path":checked(config,"compiled",&relative)?,"slug":final_slug,"changed":changed}))
}

fn mark_raw(config: &LibraryConfig, args: &Value) -> Result<Value> {
    let mut library = Library::open(config.clone())?;
    let _session = WriterSession::acquire(config)?;
    library.sync()?;
    let relative = field(args, "relative")?;
    let path = checked(config, "raw", relative)?;
    let doc = read(&path)?;
    let expected = field(args, "content_hash")?;
    if hash(doc.original.as_bytes()) != expected {
        return Err(Error::Conflict(
            "raw source changed during compilation".into(),
        ));
    }
    let mut metadata = doc.metadata.clone();
    metadata["compiled"] = args.get("compiled").cloned().unwrap_or(json!(true));
    if args.get("with_timestamp").and_then(Value::as_bool) == Some(true) {
        metadata["compiled_at"] = json!(now(&library)?);
    }
    library.write_source(
        "raw",
        relative,
        rendered(&metadata, &doc.original[doc.body_start..])?,
        Some(expected),
    )?;
    if let Some(source_key) = args.get("source_key").and_then(Value::as_str) {
        let mut keys: BTreeSet<String> = serde_json::from_value(json_file(
            &config.meta.join("compiled_sources.json"),
            json!([]),
        )?)?;
        keys.insert(source_key.into());
        library.write_compile_sources_compat(&keys.into_iter().collect::<Vec<_>>())?;
    }
    Ok(json!({"metadata":metadata,"relative":relative}))
}

fn reset_raw(config: &LibraryConfig) -> Result<Value> {
    let mut library = Library::open(config.clone())?;
    let _session = WriterSession::acquire(config)?;
    library.sync()?;
    let mut count = 0;
    for entry in fs::read_dir(&config.raw)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        for path in markdown_files(&entry.path(), false)? {
            let relative = path
                .strip_prefix(&config.raw)
                .map_err(|_| Error::Scope)?
                .to_string_lossy();
            checked(config, "raw", &relative)?;
            let doc = read(&path)?;
            let expected = hash(doc.original.as_bytes());
            let mut metadata = doc.metadata.clone();
            metadata["compiled"] = json!(false);
            library.write_source(
                "raw",
                &relative,
                rendered(&metadata, &doc.original[doc.body_start..])?,
                Some(&expected),
            )?;
            count += 1;
        }
    }
    Ok(json!(count))
}

pub fn execute(config: &LibraryConfig, operation: &str, args: &Value) -> Result<Value> {
    if serde_json::to_vec(args)?.len() > 16 * 1024 * 1024 {
        return Err(Error::Invalid("compile arguments exceed 16 MiB".into()));
    }
    let value = match operation {
        "compat_compile_find" => find_uncompiled(config)?,
        "compat_compile_existing" => list_existing(config)?,
        "compat_compile_read_raw" => read_raw(config, args)?,
        "compat_compile_load_index" => json_file(&config.meta.join("index.json"), json!([]))?,
        "compat_compile_sources" => {
            json_file(&config.meta.join("compiled_sources.json"), json!([]))?
        }
        "compat_compile_write" => write_article(config, args)?,
        "compat_compile_mark" => mark_raw(config, args)?,
        "compat_compile_reset" => reset_raw(config)?,
        "compat_compile_rebuild" => Library::open(config.clone())?.rebuild_metadata_compat()?,
        "compat_compile_backlinks" => Library::open(config.clone())?.rebuild_backlinks_compat()?,
        _ => {
            return Err(Error::Capability(
                "unknown native compile file operation".into(),
            ));
        }
    };
    Ok(
        json!({"value":value,"trace":{"core":"rust","operation":operation,"storage":"native_document_and_journal"}}),
    )
}
