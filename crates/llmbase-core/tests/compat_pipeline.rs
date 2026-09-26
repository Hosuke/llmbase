use llmbase_core::compat_pipeline::{execute_at, log_path, reduce_state};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;

fn call(base: &std::path::Path, action: &str, args: Value) -> Value {
    execute_at(base, "test", &format!("compat_pipeline_{action}"), &args).unwrap()["result"].clone()
}

#[test]
fn missing_reads_do_not_initialize_a_root() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("absent");
    assert_eq!(
        call(&base, "events", json!({"stage":"s","key":"k"})),
        json!([])
    );
    assert_eq!(
        call(&base, "state", json!({"stage":"s","key":"k"}))["status"],
        "pending"
    );
    assert_eq!(
        call(&base, "cache_get", json!({"cid":"c","content_hash":"h"})),
        json!({"hit":false})
    );
    call(&base, "cache_clear", json!({"cid":"c"}));
    assert!(!base.exists());
}

#[test]
fn torn_binary_tail_is_preserved_and_next_append_recovers() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path();
    call(
        base,
        "append",
        json!({"stage":"s","key":"法華","event":{"event":"start","ts":"forged"},"ts":"first"}),
    );
    let path = log_path(base, "s", "法華").unwrap();
    assert_eq!(path.file_stem().unwrap().len(), 64);
    let damaged = b"\n[]\n{\"event\":\"partial\"\n\xe4\xb8";
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(damaged)
        .unwrap();
    assert_eq!(
        call(base, "events", json!({"stage":"s","key":"法華"}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    call(
        base,
        "append",
        json!({"stage":"s","key":"法華","event":{"event":"ok"},"ts":"last"}),
    );
    let content = fs::read(&path).unwrap();
    assert!(content.windows(damaged.len()).any(|bytes| bytes == damaged));
    let records = call(base, "events", json!({"stage":"s","key":"法華"}));
    assert_eq!(records[0]["ts"], "first");
    assert_eq!(records[1]["ts"], "last");
    assert_eq!(
        call(base, "tail", json!({"stage":"s","key":"法華","limit":1})),
        json!([records[1].clone()])
    );
    assert!(!base.join("wiki").exists());
}

#[test]
fn last_round_metadata_and_terminal_are_reduced_from_real_events() {
    let records = json!([
        {"event":"artifact","path":"before"},
        {"event":"start","ts":"first","meta_init":{"old":true}},
        {"event":"failed","err":"historical"},
        {"event":"start","ts":42,"meta_init":{"a":1,"b":2}},
        {"event":"meta_update","ts":"ignored","a":null,"new":[1,2]},
        {"event":"artifact","path":"before"},{"event":"artifact","path":"after"},
        {"event":"artifact","path":12},{"event":"partial","reason":false},
        {"event":"failed","ts":false,"err":{"code":1}}
    ]);
    let state = reduce_state("s", "k", records.as_array().unwrap());
    assert_eq!(state["attempts"], 2);
    assert_eq!(state["status"], "failed");
    assert_eq!(state["started_at"], 42);
    assert_eq!(state["finished_at"], false);
    assert_eq!(state["last_err"], json!({"code":1}));
    assert_eq!(state["meta"], json!({"a":null,"b":2,"new":[1,2]}));
    assert_eq!(state["artifacts"], json!(["after", "before"]));
    assert_eq!(
        reduce_state(
            "s",
            "k",
            &[
                json!({"event":"start"}),
                json!({"event":"partial","reason":true})
            ]
        )["last_err"],
        "partial: True"
    );
}

#[test]
fn cache_changes_are_atomic_content_addressed_and_cid_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path();
    call(
        base,
        "cache_put",
        json!({"cid":"../../甲","content_hash":"old","output":"a\r\nb\r"}),
    );
    call(
        base,
        "cache_put",
        json!({"cid":"other","content_hash":"old","output":""}),
    );
    assert_eq!(
        call(
            base,
            "cache_get",
            json!({"cid":"../../甲","content_hash":"old"})
        ),
        json!({"hit":true,"text":"a\nb\n"})
    );
    assert_eq!(
        call(
            base,
            "cache_get",
            json!({"cid":"../../甲","content_hash":"new"})
        ),
        json!({"hit":false})
    );
    call(base, "cache_clear", json!({"cid":"../../甲"}));
    assert_eq!(
        call(
            base,
            "cache_get",
            json!({"cid":"other","content_hash":"old"})
        ),
        json!({"hit":true,"text":""})
    );
    let dirs: Vec<_> = fs::read_dir(base.join(".chunk_cache"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(dirs.len(), 1);
    assert_eq!(dirs[0].file_name().unwrap().len(), 64);
    let entries: Vec<_> = fs::read_dir(&dirs[0])
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].file_stem().unwrap().len(), 64);
}

#[test]
fn cache_invalid_utf8_is_not_a_miss() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path();
    call(
        base,
        "cache_put",
        json!({"cid":"c","content_hash":"h","output":"ok"}),
    );
    let path = base
        .join(".chunk_cache")
        .join(llmbase_core::document::hash(b"c"))
        .join(format!("{}.md", llmbase_core::document::hash(b"h")));
    fs::write(path, [b'x', 255]).unwrap();
    let result = call(base, "cache_get", json!({"cid":"c","content_hash":"h"}));
    assert_eq!(result["invalid_utf8"], json!([120, 255]));
    assert_eq!(result["start"], 1);
}

#[test]
fn separate_threads_append_complete_records_without_interleaving() {
    let temp = tempfile::tempdir().unwrap();
    std::thread::scope(|scope| {
        for worker in 0..4 {
            let base = temp.path();
            scope.spawn(move|| {
            for i in 0..25 { call(base,"append",json!({"stage":"s","key":"k","event":{"event":"work","worker":worker,"i":i},"ts":"now"})); }
        });
        }
    });
    assert_eq!(
        call(temp.path(), "events", json!({"stage":"s","key":"k"}))
            .as_array()
            .unwrap()
            .len(),
        100
    );
}

#[test]
fn stage_and_cache_subdirectories_cannot_escape_configured_roots() {
    let temp = tempfile::tempdir().unwrap();
    for stage in ["..", "bad/name", "", "bad\n"] {
        assert!(
            execute_at(
                temp.path(),
                "test",
                "compat_pipeline_events",
                &json!({"stage":stage,"key":"k"})
            )
            .is_err()
        );
    }
    assert!(
        execute_at(
            temp.path(),
            "test",
            "compat_pipeline_cache_put",
            &json!({"subdir":"../outside","cid":"c","content_hash":"h","output":"x"})
        )
        .is_err()
    );
}

#[test]
fn stage_kernel_leases_are_scoped_and_survive_pidfile_removal() {
    use llmbase_core::compat_pipeline::StageLease;
    let temp = tempfile::tempdir().unwrap();
    let lease = StageLease::acquire(temp.path(), "s", "a").unwrap().unwrap();
    let other = StageLease::acquire(temp.path(), "s", "b").unwrap().unwrap();
    assert!(lease.owned_here() && other.owned_here());
    assert!(
        StageLease::acquire(temp.path(), "s", "a")
            .unwrap()
            .is_none()
    );
    let pidfile = log_path(temp.path(), "s", "a")
        .unwrap()
        .with_extension("lock");
    fs::write(&pidfile, b"fake operator pidfile").unwrap();
    fs::remove_file(pidfile).unwrap();
    assert!(
        StageLease::acquire(temp.path(), "s", "a")
            .unwrap()
            .is_none()
    );
    drop(lease);
    assert!(
        StageLease::acquire(temp.path(), "s", "a")
            .unwrap()
            .is_some()
    );
}

#[test]
fn arbitrary_integer_and_metadata_insertion_order_survive_log_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let record: Value = serde_json::from_str(
        r#"{"event":"partial","reason":{"z":1208925819614629174706176,"a":true}}"#,
    )
    .unwrap();
    call(
        temp.path(),
        "append",
        json!({"stage":"s","key":"k","event":{"event":"start"},"ts":"t"}),
    );
    call(
        temp.path(),
        "append",
        json!({"stage":"s","key":"k","event":record,"ts":"t"}),
    );
    let result = call(temp.path(), "state", json!({"stage":"s","key":"k"}));
    assert_eq!(
        result["last_err"],
        "partial: {'z': 1208925819614629174706176, 'a': True}"
    );
}

#[test]
fn native_extended_json_transport_preserves_nonfinite_records_and_reduction() {
    use llmbase_core::compat_pipeline::execute_json_at;
    let temp = tempfile::tempdir().unwrap();
    let first = r#"{"stage":"s","key":"k","ts":"first","event":{"event":"start","meta_init":{"nan":NaN,"inf":Infinity,"negative":-Infinity,"literal":"NaN Infinity"}}}"#;
    execute_json_at(temp.path(), "test", "compat_pipeline_append", first).unwrap();
    let second = r#"{"stage":"s","key":"k","ts":"last","event":{"event":"partial","reason":[NaN,Infinity,-Infinity]}}"#;
    execute_json_at(temp.path(), "test", "compat_pipeline_append", second).unwrap();
    let args = r#"{"stage":"s","key":"k","limit":1}"#;
    let records = execute_json_at(temp.path(), "test", "compat_pipeline_events", args).unwrap();
    assert!(records.contains("\"nan\":NaN"));
    assert!(records.contains("\"inf\":Infinity"));
    assert!(records.contains("\"negative\":-Infinity"));
    assert!(records.contains("\"literal\":\"NaN Infinity\""));
    let state = execute_json_at(temp.path(), "test", "compat_pipeline_state", args).unwrap();
    assert!(state.contains("partial: [nan, inf, -inf]"));
    let tail = execute_json_at(temp.path(), "test", "compat_pipeline_tail", args).unwrap();
    assert!(!tail.contains("\"event\":\"start\""));
    let bytes = fs::read_to_string(log_path(temp.path(), "s", "k").unwrap()).unwrap();
    assert_eq!(
        bytes,
        "{\"event\":\"start\",\"meta_init\":{\"nan\":NaN,\"inf\":Infinity,\"negative\":-Infinity,\"literal\":\"NaN Infinity\"},\"ts\":\"first\"}\n{\"event\":\"partial\",\"reason\":[NaN,Infinity,-Infinity],\"ts\":\"last\"}\n"
    );
}
