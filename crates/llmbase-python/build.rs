use std::path::{Path, PathBuf};
use std::process::Command;

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn commit(value: &str) -> Option<String> {
    let value = value.trim();
    ((value.len() == 40 || value.len() == 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}

fn rerun_for_git(root: &Path) {
    for argument in ["HEAD", "index", "packed-refs"] {
        if let Some(path) = git(root, &["rev-parse", "--git-path", argument]) {
            let path = PathBuf::from(path);
            println!(
                "cargo:rerun-if-changed={}",
                if path.is_absolute() {
                    path
                } else {
                    root.join(path)
                }
                .display()
            );
        }
    }
    if let Some(reference) = git(root, &["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git(root, &["rev-parse", "--git-path", &reference])
    {
        let path = PathBuf::from(path);
        println!(
            "cargo:rerun-if-changed={}",
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
            .display()
        );
    }
    if let Some(files) = git(root, &["ls-files"]) {
        for file in files.lines().filter(|file| !file.is_empty()) {
            println!("cargo:rerun-if-changed={}", root.join(file).display());
        }
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=LLMBASE_BUILD_COMMIT");
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .expect("llmbase-python must be inside the workspace")
        .canonicalize()
        .expect("workspace root must exist");
    let source_commit = root.join("SOURCE_COMMIT");
    println!("cargo:rerun-if-changed={}", source_commit.display());

    let own_git = git(&root, &["rev-parse", "--show-toplevel"])
        .and_then(|path| PathBuf::from(path).canonicalize().ok())
        .is_some_and(|path| path == root);
    let head = own_git
        .then(|| git(&root, &["rev-parse", "HEAD"]).and_then(|value| commit(&value)))
        .flatten();
    let explicit = std::env::var_os("LLMBASE_BUILD_COMMIT").map(|value| {
        let value = value
            .to_str()
            .expect("LLMBASE_BUILD_COMMIT must be valid Unicode");
        commit(value).expect("LLMBASE_BUILD_COMMIT must be 40 or 64 hexadecimal characters")
    });
    if let (Some(explicit), Some(head)) = (&explicit, &head)
        && explicit != head
    {
        panic!("LLMBASE_BUILD_COMMIT does not match the workspace Git HEAD");
    }
    let fallback = std::fs::read_to_string(&source_commit)
        .ok()
        .and_then(|value| commit(&value));
    let build_commit = explicit
        .or(head)
        .or(fallback)
        .unwrap_or_else(|| "unknown".into());
    let build_dirty = if own_git {
        rerun_for_git(&root);
        let status = git(&root, &["status", "--porcelain", "--untracked-files=no"])
            .expect("failed to inspect tracked workspace changes");
        if status.is_empty() { "false" } else { "true" }
    } else {
        "unknown"
    };
    println!("cargo:rustc-env=LLMBASE_BUILD_COMMIT={build_commit}");
    println!("cargo:rustc-env=LLMBASE_BUILD_DIRTY={build_dirty}");

    if std::env::var_os("CARGO_FEATURE_EXTENSION_MODULE").is_some() {
        pyo3_build_config::add_extension_module_link_args();
    }
}
