#!/usr/bin/env python3
"""Verify that a source archive can reproduce the native wheel build."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import tarfile
import tomllib


REQUIRED = (
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "SOURCE_COMMIT",
    "docs/rust-migration/contracts/baseline.json",
    "crates/llmbase-core/Cargo.toml",
    "crates/llmbase-model/Cargo.toml",
    "crates/llmbase-cli/Cargo.toml",
    "crates/llmbase-python/Cargo.toml",
    "crates/llmbase-python/build.rs",
    "examples/rust-extension/Cargo.toml",
    "examples/rust-extension/src/lib.rs",
    "examples/rust-extension/src/main.rs",
    "examples/rust-extension/tests/cli.rs",
    "tests/rust_migration/test_build_provenance.py",
    "tests/rust_migration/test_http_public_projections.py",
    "scripts/release/test_installed_wheel.py",
)


def member_bytes(archive: tarfile.TarFile, name: str) -> bytes:
    member = archive.getmember(name)
    source = archive.extractfile(member)
    if source is None:
        raise AssertionError(f"sdist member is not a regular file: {name}")
    return source.read()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=Path)
    parser.add_argument("--expected-commit", required=True)
    parser.add_argument("--expected-version", required=True)
    parser.add_argument("--receipt", required=True, type=Path)
    args = parser.parse_args()

    expected_commit = args.expected_commit.lower()
    assert len(expected_commit) in {40, 64} and all(c in "0123456789abcdef" for c in expected_commit)
    digest = hashlib.sha256(args.archive.read_bytes()).hexdigest()
    with tarfile.open(args.archive, "r:gz") as archive:
        files = {member.name for member in archive.getmembers() if member.isfile()}
        roots = {PurePosixPath(name).parts[0] for name in files}
        assert len(roots) == 1, f"sdist must have one root directory, found {sorted(roots)}"
        root = roots.pop()
        missing = [name for name in REQUIRED if f"{root}/{name}" not in files]
        assert not missing, f"sdist is missing build inputs: {missing}"
        assert not any(".git" in PurePosixPath(name).parts for name in files)
        commit = member_bytes(archive, f"{root}/SOURCE_COMMIT").decode().strip().lower()
        project = tomllib.loads(member_bytes(archive, f"{root}/pyproject.toml").decode())

    version = project["project"]["version"]
    assert commit == expected_commit
    assert version == args.expected_version
    receipt = {
        "artifact": args.archive.name,
        "sha256": digest,
        "version": version,
        "build_commit": commit,
        "platform": "source",
    }
    args.receipt.parent.mkdir(parents=True, exist_ok=True)
    args.receipt.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(json.dumps(receipt, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
