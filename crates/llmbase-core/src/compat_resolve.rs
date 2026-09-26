//! Local Python alias helpers. Ordered pairs preserve the legacy first-match
//! cascade independently of the ambiguity-rejecting indexed catalog API.
use crate::{Error, Result, compat_kb};
use ferrous_opencc::{OpenCC, config::BuiltinConfig};
use regex::Regex;
use serde_json::{Value, json};
use std::{io::Read, path::Path, sync::LazyLock};

static FUZZY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\p{L}\p{N}_]").unwrap());
static T2S: LazyLock<std::result::Result<OpenCC, String>> =
    LazyLock::new(|| OpenCC::from_config(BuiltinConfig::T2s).map_err(|e| e.to_string()));
static S2T: LazyLock<std::result::Result<OpenCC, String>> =
    LazyLock::new(|| OpenCC::from_config(BuiltinConfig::S2t).map_err(|e| e.to_string()));
pub fn normalize(text: &str) -> String {
    text.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
        .to_lowercase()
}
pub fn fuzzy(text: &str) -> String {
    let mut text = FUZZY.replace_all(&normalize(text), "").into_owned();
    for stop in ["the", "of", "in", "on", "and", "for", "its"] {
        text = text.replace(stop, "");
    }
    text
}
pub fn convert(text: &str, traditional: bool) -> Result<String> {
    Ok((if traditional { &*S2T } else { &*T2S })
        .as_ref()
        .map_err(|e| Error::Capability(e.clone()))?
        .convert(text))
}
pub fn pairs(value: &Value) -> Result<Vec<(String, Value)>> {
    serde_json::from_value(value.clone()).map_err(Into::into)
}

pub fn read_pairs(path: &Path) -> Result<Vec<(String, Value)>> {
    struct Ordered(Vec<(String, Value)>);
    impl<'de> serde::Deserialize<'de> for Ordered {
        fn deserialize<D: serde::Deserializer<'de>>(
            deserializer: D,
        ) -> std::result::Result<Self, D::Error> {
            struct Visitor;
            impl<'de> serde::de::Visitor<'de> for Visitor {
                type Value = Ordered;
                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                    formatter.write_str("an alias mapping")
                }
                fn visit_map<M: serde::de::MapAccess<'de>>(
                    self,
                    mut map: M,
                ) -> std::result::Result<Self::Value, M::Error> {
                    let mut pairs = Vec::<(String, Value)>::new();
                    while let Some((key, value)) = map.next_entry::<String, Value>()? {
                        if let Some((_, old)) = pairs.iter_mut().find(|(name, _)| name == &key) {
                            *old = value;
                        } else {
                            pairs.push((key, value));
                        }
                    }
                    Ok(Ordered(pairs))
                }
            }
            deserializer.deserialize_map(Visitor)
        }
    }
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(Error::Invalid("aliases exceed 4 MiB".into()));
    }
    Ok(serde_json::from_slice::<Ordered>(&bytes)?.0)
}
pub fn resolve(target: &str, aliases: &[(String, Value)], conversions: &[String]) -> Option<Value> {
    if target.is_empty() {
        return None;
    }
    let key = normalize(target);
    for key in [key.clone(), key.replace(' ', "-"), key.replace(' ', "")]
        .into_iter()
        .chain(conversions.iter().map(|s| normalize(s)))
    {
        if let Some((_, value)) = aliases.iter().find(|(name, _)| name == &key) {
            return Some(value.clone());
        }
    }
    let key = fuzzy(target);
    aliases
        .iter()
        .find(|(name, _)| fuzzy(name) == key)
        .map(|(_, value)| value.clone())
}
fn register(aliases: &mut Vec<(String, Value)>, name: &str, value: &Value) {
    let name = normalize(name);
    if !name.is_empty() && !aliases.iter().any(|(key, _)| key == &name) {
        aliases.push((name, value.clone()));
    }
}
fn cjk(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}'))
}
fn variants(aliases: &mut Vec<(String, Value)>) -> Result<()> {
    let mut new = Vec::<(String, Value)>::new();
    for (key, slug) in aliases.iter().filter(|(key, _)| cjk(key)) {
        for name in [convert(key, false)?, convert(key, true)?] {
            let name = normalize(&name);
            if !name.is_empty() && !aliases.iter().any(|(key, _)| key == &name) {
                // Legacy variants are accumulated separately; later values win
                // among newly generated keys, while existing keys never change.
                if let Some((_, value)) = new.iter_mut().find(|(key, _)| key == &name) {
                    *value = slug.clone();
                } else {
                    new.push((name, slug.clone()));
                }
            }
        }
    }
    aliases.extend(new);
    Ok(())
}
fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid(format!("{key} must be a string")))
}
fn target(args: &Value) -> Result<&str> {
    if args["target"].is_null() {
        Ok("")
    } else {
        text(args, "target")
    }
}

/// Filesystem actions are exposed only through the trusted local Python ABI,
/// never generic HTTP operation dispatch or a caller-selected remote path.
pub fn transform(action: &str, args: &Value) -> Result<Value> {
    match action {
        "resolve_initial" => {
            let aliases = pairs(&args["aliases"])?;
            let target = target(args)?;
            if target.is_empty() {
                return Ok(json!({"found":true,"value":null}));
            }
            let key = normalize(target);
            for key in [key.clone(), key.replace(' ', "-"), key.replace(' ', "")] {
                if let Some((_, value)) = aliases.iter().find(|(name, _)| name == &key) {
                    return Ok(json!({"found":true,"value":value}));
                }
            }
            Ok(json!({"found":false}))
        }
        "normalize" => Ok(json!(normalize(text(args, "text")?))),
        "fuzzy" => Ok(json!(fuzzy(text(args, "text")?))),
        "convert" => Ok(json!(convert(
            text(args, "text")?,
            args["traditional"] == true
        )?)),
        "resolve" => {
            let aliases = pairs(&args["aliases"])?;
            let target = target(args)?;
            let conversions = args
                .get("conversions")
                .map(|v| serde_json::from_value::<Vec<String>>(v.clone()))
                .transpose()?
                .unwrap_or_default();
            Ok(resolve(target, &aliases, &conversions).unwrap_or(Value::Null))
        }
        "register" => {
            let mut aliases = pairs(&args["aliases"])?;
            register(&mut aliases, text(args, "name")?, &args["slug"]);
            Ok(json!(aliases))
        }
        "variants" => {
            let mut aliases = pairs(&args["aliases"])?;
            variants(&mut aliases)?;
            Ok(json!(aliases))
        }
        "build" => {
            let root = Path::new(text(args, "concepts_dir")?);
            let mut aliases = Vec::new();
            let files = compat_kb::markdown_files(root, true)?;
            if files.len() > 20_000 {
                return Err(Error::Invalid("alias build exceeds 20000 articles".into()));
            }
            for path in files {
                let doc = compat_kb::read(&path)?;
                if args["public_only"] == true
                    && (doc.metadata["private"] == true
                        || doc
                            .metadata
                            .get("visibility")
                            .is_some_and(|v| v != "public"))
                {
                    continue;
                }
                let slug = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| Error::Invalid("article name must be UTF-8".into()))?;
                let value = json!(slug);
                let title = doc
                    .metadata
                    .get("title")
                    .map(|v| {
                        v.as_str()
                            .ok_or_else(|| Error::Invalid("article title must be a string".into()))
                    })
                    .transpose()?
                    .unwrap_or(slug);
                register(&mut aliases, slug, &value);
                register(&mut aliases, title, &value);
                for part in title.split('/') {
                    register(&mut aliases, part, &value);
                }
                if let Some(old) = doc.metadata.get("merged_from") {
                    for name in old
                        .as_array()
                        .ok_or_else(|| Error::Invalid("merged_from must be an array".into()))?
                    {
                        register(
                            &mut aliases,
                            name.as_str().ok_or_else(|| {
                                Error::Invalid("merged_from entries must be strings".into())
                            })?,
                            &value,
                        );
                    }
                }
            }
            if args["cjk"] != false {
                variants(&mut aliases)?;
            }
            Ok(json!(aliases))
        }
        "load" => {
            let path = Path::new(text(args, "meta_dir")?).join("aliases.json");
            Ok(json!(read_pairs(&path)?))
        }
        _ => Err(Error::Capability("unknown local alias helper".into())),
    }
}
