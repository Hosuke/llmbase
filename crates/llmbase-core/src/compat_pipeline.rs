//! Legacy pipeline persistence and reduction. No database, Python, or model dependency.
use crate::{Error, LibraryConfig, Result, config::scoped_path, document::hash};
use fs2::FileExt;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;
#[path = "compat_pipeline_json.rs"]
mod extended_json;
use extended_json::Codec;

static CALLS: AtomicU64 = AtomicU64::new(0);

/// Per-stage kernel lease. The stable inode is never removed by stale-PID recovery.
/// Closing the descriptor releases only this process' reference, including after fork.
pub struct StageLease {
    _file: File,
    owner_pid: u32,
}

impl StageLease {
    pub fn acquire(base: &Path, stage: &str, key: &str) -> Result<Option<Self>> {
        let path = log_path(base, stage, key)?.with_extension("lock.kernel");
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| Error::Invalid("stage lock has no parent".into()))?,
        )?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(Self {
                _file: file,
                owner_pid: std::process::id(),
            })),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn owned_here(&self) -> bool {
        self.owner_pid == std::process::id()
    }
}

fn field<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid(format!("{key} must be a string")))
}

pub fn log_path(base: &Path, stage: &str, key: &str) -> Result<PathBuf> {
    let mut chars = stage.chars();
    if !chars
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        || !chars.all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
    {
        return Err(Error::Invalid(
            "invalid stage name: expected [A-Za-z0-9_][A-Za-z0-9_.-]*".into(),
        ));
    }
    scoped_path(
        base,
        &format!(".pipeline/{stage}/{}.jsonl", hash(key.as_bytes())),
    )
}

fn append(path: &Path, event: &Value, ts: &Value, codec: &Codec) -> Result<()> {
    let mut record = event
        .as_object()
        .cloned()
        .ok_or_else(|| Error::Invalid("event must be a mapping".into()))?;
    record.insert("ts".into(), ts.clone());
    let mut bytes = codec.encode(&Value::Object(record))?.into_bytes();
    bytes.push(b'\n');
    let parent = path
        .parent()
        .ok_or_else(|| Error::Invalid("log has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(path)?;
    // Same flock inode as the old Python appender; the lock is released on any error.
    file.lock_exclusive()?;
    if file.seek(SeekFrom::End(0))? > 0 {
        file.seek(SeekFrom::End(-1))?;
        let mut last = [0];
        file.read_exact(&mut last)?;
        if last[0] != b'\n' {
            file.write_all(b"\n")?;
        }
    }
    file.write_all(&bytes)?;
    file.sync_all()?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn events(path: &Path, codec: &mut Codec) -> Result<Vec<Value>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => return Err(error.into()),
    };
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    let mut result = Vec::new();
    while reader.read_until(b'\n', &mut line)? != 0 {
        if let Ok(text) = std::str::from_utf8(&line) {
            match codec.parse(text.trim()) {
                Ok(value) if value.is_object() => result.push(value),
                _ => (),
            }
        }
        line.clear();
    }
    Ok(result)
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}

// Python str/repr for JSON values used by interrupted/partial error summaries.
fn python_string(value: &Value, nested: bool, codec: &Codec) -> String {
    match value {
        Value::Null => "None".into(),
        Value::Bool(v) => if *v { "True" } else { "False" }.into(),
        Value::String(v) if !nested => v.clone(),
        Value::String(v) => {
            let quote = if v.contains('\'') && !v.contains('"') {
                '"'
            } else {
                '\''
            };
            let mut output = String::new();
            output.push(quote);
            for c in v.chars() {
                match c {
                    '\\' => output.push_str("\\\\"),
                    '\n' => output.push_str("\\n"),
                    '\r' => output.push_str("\\r"),
                    '\t' => output.push_str("\\t"),
                    c if c == quote => {
                        output.push('\\');
                        output.push(c);
                    }
                    c if c.is_control() && (c as u32) < 256 => {
                        output.push_str(&format!("\\x{:02x}", c as u32))
                    }
                    c => output.push(c),
                }
            }
            output.push(quote);
            output
        }
        Value::Array(v) => format!(
            "[{}]",
            v.iter()
                .map(|v| python_string(v, true, codec))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Object(v) => format!(
            "{{{}}}",
            v.iter()
                .map(|(k, v)| format!(
                    "{}: {}",
                    python_string(&json!(k), true, codec),
                    python_string(v, true, codec)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Number(number) => codec.python_number(number),
    }
}

pub fn reduce_state(stage: &str, key: &str, events: &[Value]) -> Value {
    reduce_state_with_codec(stage, key, events, &Codec::default())
}

fn reduce_state_with_codec(stage: &str, key: &str, events: &[Value], codec: &Codec) -> Value {
    let starts: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| e.get("event").and_then(Value::as_str) == Some("start"))
        .map(|(i, _)| i)
        .collect();
    let artifacts: BTreeSet<&str> = events
        .iter()
        .filter(|e| e.get("event").and_then(Value::as_str) == Some("artifact"))
        .filter_map(|e| e.get("path").and_then(Value::as_str))
        .collect();
    let mut state = json!({"stage":stage,"key":key,"status":"pending","attempts":starts.len(),
        "started_at":null,"finished_at":null,"last_err":null,"artifacts":artifacts,"meta":{}});
    let Some(last) = starts.last() else {
        return state;
    };
    let round = &events[*last..];
    state["started_at"] = round[0].get("ts").cloned().unwrap_or(Value::Null);
    state["status"] = json!("running");
    if let Some(terminal) = round.iter().rev().find(|e| {
        matches!(
            e.get("event").and_then(Value::as_str),
            Some("ok" | "failed" | "partial" | "interrupted")
        )
    }) {
        state["status"] = terminal["event"].clone();
        state["finished_at"] = terminal.get("ts").cloned().unwrap_or(Value::Null);
        state["last_err"] = match terminal["event"].as_str() {
            Some("failed") => terminal.get("err").cloned().unwrap_or(Value::Null),
            Some("interrupted") => json!(format!(
                "interrupted: {}",
                python_string(
                    terminal.get("by").unwrap_or(&json!("unknown")),
                    false,
                    codec
                )
            )),
            Some("partial") => match terminal.get("reason").filter(|v| truthy(v)) {
                Some(reason) => json!(format!("partial: {}", python_string(reason, false, codec))),
                None => json!("partial"),
            },
            _ => Value::Null,
        };
    }
    let mut meta: Map<String, Value> = round[0]
        .get("meta_init")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for event in &round[1..] {
        if event.get("event").and_then(Value::as_str) == Some("meta_update")
            && let Some(values) = event.as_object()
        {
            for (key, value) in values {
                if key != "event" && key != "ts" {
                    meta.insert(key.clone(), value.clone());
                }
            }
        }
    }
    state["meta"] = Value::Object(meta);
    state
}

fn cache_root(base: &Path, args: &Value) -> Result<PathBuf> {
    // Trusted local bridge may select a root directly by passing it as `base` with
    // subdir="."; remote Application arguments remain relative to its configured base.
    let subdir = args
        .get("subdir")
        .and_then(Value::as_str)
        .unwrap_or(".chunk_cache");
    if subdir == "." {
        return Ok(base.to_owned());
    }
    scoped_path(base, subdir)
}

fn cache_lock(root: &Path) -> Result<File> {
    let parent = root
        .parent()
        .ok_or_else(|| Error::Invalid("cache has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let name = root
        .file_name()
        .ok_or_else(|| Error::Invalid("cache has no name".into()))?
        .to_string_lossy();
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(parent.join(format!(".{name}.llmbase-cache.lock")))?;
    lock.lock_exclusive()?;
    Ok(lock)
}

fn cache(base: &Path, action: &str, args: &Value) -> Result<Value> {
    let root = cache_root(base, args)?;
    let cid = hash(field(args, "cid")?.as_bytes());
    let directory = scoped_path(&root, &cid)?;
    if action == "cache_clear" {
        if !directory.exists() {
            return Ok(Value::Null);
        }
        let _lock = cache_lock(&root)?;
        match fs::remove_dir_all(&directory) {
            Ok(()) => {
                if root.is_dir() {
                    File::open(&root)?.sync_all()?;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        return Ok(Value::Null);
    }
    let path = scoped_path(
        &directory,
        &format!("{}.md", hash(field(args, "content_hash")?.as_bytes())),
    )?;
    if action == "cache_get" {
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(json!({"hit":false})),
        };
        return match String::from_utf8(bytes) {
            Ok(text) => Ok(json!({"hit":true,"text":text.replace("\r\n","\n").replace('\r',"\n")})),
            Err(error) => {
                let utf8 = error.utf8_error();
                Ok(
                    json!({"invalid_utf8":error.into_bytes(),"start":utf8.valid_up_to(),"length":utf8.error_len()}),
                )
            }
        };
    }
    let output = field(args, "output")?;
    let _lock = cache_lock(&root)?;
    fs::create_dir_all(&directory)?;
    let temp = directory.join(format!(".llmbase-{}.tmp", Uuid::new_v4()));
    let outcome = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(output.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp, &path)?;
        File::open(&directory)?.sync_all()?;
        Ok(())
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(&temp);
    }
    outcome?;
    Ok(Value::Null)
}

pub fn execute_at(base: &Path, library_id: &str, operation: &str, args: &Value) -> Result<Value> {
    let mut codec = Codec::default();
    let value = execute_with_codec(base, library_id, operation, args, &mut codec)?;
    codec.finite_value(value)
}

/// Local Python JSON transport, including NaN and signed infinities.
pub fn execute_json_at(
    base: &Path,
    library_id: &str,
    operation: &str,
    args_json: &str,
) -> Result<String> {
    let mut codec = Codec::default();
    let args = codec.parse(args_json)?;
    let value = execute_with_codec(base, library_id, operation, &args, &mut codec)?;
    codec.encode(&value)
}

fn execute_with_codec(
    base: &Path,
    library_id: &str,
    operation: &str,
    args: &Value,
    codec: &mut Codec,
) -> Result<Value> {
    crate::config::validate_id(library_id)?;
    let action = operation
        .strip_prefix("compat_pipeline_")
        .ok_or_else(|| Error::Invalid("unknown pipeline operation".into()))?;
    let value = match action {
        "cache_get" | "cache_put" | "cache_clear" => cache(base, action, args)?,
        "append" | "events" | "tail" | "state" => {
            let stage = field(args, "stage")?;
            let key = field(args, "key")?;
            let path = log_path(base, stage, key)?;
            if action == "append" {
                append(
                    &path,
                    args.get("event").unwrap_or(&Value::Null),
                    args.get("ts")
                        .ok_or_else(|| Error::Invalid("authoritative ts is required".into()))?,
                    codec,
                )?;
                Value::Null
            } else {
                let events = if let Some(supplied) = args.get("events") {
                    supplied
                        .as_array()
                        .cloned()
                        .ok_or_else(|| Error::Invalid("events must be an array".into()))?
                } else {
                    events(&path, codec)?
                };
                match action {
                    "state" => reduce_state_with_codec(stage, key, &events, codec),
                    "tail" => {
                        let limit = args
                            .get("limit")
                            .and_then(Value::as_i64)
                            .unwrap_or(50)
                            .max(0) as usize;
                        json!(&events[events.len().saturating_sub(limit)..])
                    }
                    _ => json!(events),
                }
            }
        }
        _ => return Err(Error::Invalid("unknown pipeline operation".into())),
    };
    let storage = if args.get("events").is_some() {
        "callback_events"
    } else {
        "native_pipeline_files"
    };
    Ok(
        json!({"result":value,"trace":{"core":"rust","operation":operation,"library_id":library_id,
        "storage":storage,"sequence":CALLS.fetch_add(1,Ordering::Relaxed)+1}}),
    )
}

pub fn execute(config: &LibraryConfig, operation: &str, args: &Value) -> Result<Value> {
    execute_at(&config.base, &config.library_id, operation, args)
}
