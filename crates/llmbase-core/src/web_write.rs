//! Accepted deterministic Web mutations; shared journals own all filesystem writes.
use crate::{Error, Library, Result, Source, config::scoped_path, store::WriterLock};
use serde_json::{Value, json};
use std::fs;

pub fn execute(
    library: &mut Library,
    operation: &str,
    args: &Value,
    private: bool,
) -> Result<Value> {
    if !args.is_object() {
        return Err(Error::Invalid(
            "web mutation arguments must be an object".into(),
        ));
    }
    if matches!(operation, "web_write_trail_save" | "web_write_trail_delete") {
        if !private {
            return Err(Error::Scope);
        }
        return mutate_trails(library, operation, args);
    }
    let sources = {
        let _lock = WriterLock::acquire(&library.config)?;
        crate::web_read::live_sources(library, private).map_err(|error| match error {
            Error::NotFound(_) => {
                Error::Conflict("source visibility changed before web mutation".into())
            }
            error => error,
        })?
    };
    match operation {
        "web_write_article_delete" => {
            let slug = string(args, "slug")?;
            if slug.contains('/') || slug.contains('\\') {
                return Err(Error::Invalid("delete requires one article slug".into()));
            }
            scoped_path(&library.config.concepts, &format!("{slug}.md"))?;
            let source = sources
                .iter()
                .find(|s| s.kind == "compiled" && s.slug == slug)
                .ok_or_else(|| Error::NotFound("article".into()))?;
            library.delete_source(&source.source_id, &source.content_hash)?;
            Ok(json!({"status":"ok","deleted":slug}))
        }
        "web_write_wiki_clean" => {
            let mut slugs = Vec::new();
            let mut failures = Vec::new();
            for source in sources
                .iter()
                .filter(|s| s.kind == "compiled" && legacy_garbage(s))
            {
                match library.delete_source(&source.source_id,&source.content_hash){
                    Ok(_)=>slugs.push(source.slug.clone()),
                    Err(_)=>failures.push(json!({"slug":source.slug,"message":"source changed or journaled delete failed"})),
                }
            }
            if !slugs.is_empty() && library.rebuild_metadata().is_err() {
                failures.push(json!({"message":"metadata rebuild failed after deletions"}));
            }
            if slugs.is_empty() && !failures.is_empty() {
                return Err(Error::Conflict(
                    "no requested cleanup deletion completed".into(),
                ));
            }
            Ok(
                json!({"status":if failures.is_empty(){"ok"}else{"partial"},"removed":slugs.len(),"slugs":slugs,"failures":failures}),
            )
        }
        "web_write_taxonomy_update" => {
            let document = args
                .get("document")
                .cloned()
                .ok_or_else(|| Error::Invalid("document required".into()))?;
            let mut result = library.set_taxonomy(document, private)?;
            result["status"] = json!("ok");
            Ok(result)
        }
        _ => Err(Error::Capability(format!(
            "unknown web write operation: {operation}"
        ))),
    }
}
fn string<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid(format!("{key} must be a string")))
}
fn legacy_garbage(source: &Source) -> bool {
    let title = source
        .metadata
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("");
    let summary = source
        .metadata
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("");
    let body = source.content[source.body_start..].trim();
    title.contains("English Title / 中文标题")
        || summary.contains("One-line summary in English")
        || summary.contains("The user says")
        || body.contains("has not been fully written")
        || body.contains("has not been written yet")
        || body.contains("尚未完成撰写")
        || body.chars().count() < 50
}
fn mutate_trails(library: &mut Library, operation: &str, args: &Value) -> Result<Value> {
    let _lock = WriterLock::acquire(&library.config)?;
    library.recover_projections_locked()?;
    let path = scoped_path(&library.config.meta, "trails.json")?;
    let previous = match fs::metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
        Ok(meta) => {
            if !meta.is_file() || meta.len() > 4 * 1024 * 1024 {
                return Err(Error::Invalid(
                    "trails exceed 4 MiB or are not a file".into(),
                ));
            }
            Some(fs::read(path)?)
        }
    };
    let mut document: Value = match &previous {
        Some(bytes) => serde_json::from_slice(bytes)?,
        None => json!({"trails":[]}),
    };
    let trails = document
        .get_mut("trails")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| Error::Invalid("invalid trails document".into()))?;
    if trails.len() > 10_000 {
        return Err(Error::Invalid("too many trails".into()));
    }
    let output = if operation == "web_write_trail_delete" {
        let id = string(args, "trail_id")?;
        trails.retain(|trail| trail.get("id").and_then(Value::as_str) != Some(id));
        json!({"status":"ok"})
    } else {
        let mut step = args
            .get("step")
            .filter(|s| !s.is_null())
            .cloned()
            .unwrap_or(json!({}));
        if !step.is_object() {
            return Err(Error::Invalid("step must be a mapping".into()));
        }
        let now: String =
            library
                .db
                .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
                    r.get(0)
                })?;
        step["ts"] = json!(now);
        let trail = if let Some(id) = args.get("trail_id").filter(|v| !v.is_null()) {
            let id = id
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| Error::Invalid("trail_id must be a nonempty string".into()))?;
            let trail = trails
                .iter_mut()
                .find(|t| t["id"] == id)
                .ok_or_else(|| Error::NotFound("trail".into()))?;
            let steps = trail
                .get_mut("steps")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| Error::Invalid("invalid trail steps".into()))?;
            if steps.len() >= 10_000 {
                return Err(Error::Invalid("too many trail steps".into()));
            }
            steps.push(step);
            trail["updated"] = json!(now);
            trail.clone()
        } else {
            let name = args
                .get("name")
                .filter(|v| !v.is_null())
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| Error::Invalid("name must be a string".into()))
                })
                .transpose()?
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Trail {}", trails.len() + 1));
            if name.len() > 4096 {
                return Err(Error::Invalid("trail name exceeds 4096 bytes".into()));
            }
            let steps = if step
                .get("type")
                .is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()))
            {
                vec![step]
            } else {
                Vec::new()
            };
            let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_owned();
            let trail = json!({"id":id,"name":name,"created":now,"updated":now,"steps":steps});
            trails.push(trail.clone());
            trail
        };
        json!({"trail":trail})
    };
    library.write_trails_compat_locked(previous.as_deref(), &document)?;
    Ok(output)
}
