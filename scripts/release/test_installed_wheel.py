#!/usr/bin/env python3
"""Validate an installed native wheel without importing from its source tree."""
from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


SENSITIVE_ENV = (
    "PYTHONPATH",
    "OPENAI_API_KEY",
    "LLM_API_KEY",
    "LLMBASE_API_KEY",
    "LLMBASE_EMBEDDING_API_KEY",
    "LLMBASE_BASE_URL",
    "LLMBASE_MODEL",
    "LLMBASE_EMBEDDING_BASE_URL",
    "LLMBASE_EMBEDDING_MODEL",
)


def installed_backend() -> tuple[dict, str]:
    import llmwiki
    from llmwiki import _core, _native

    prefix = Path(sys.prefix).resolve()
    for module in (llmwiki, _core):
        location = Path(module.__file__).resolve()
        assert location.is_relative_to(prefix), f"source-tree import: {location}"
    backend = _native.backend_info()
    expected_commit = os.environ["LLMBASE_EXPECT_BUILD_COMMIT"].lower()
    expected_version = os.environ["LLMBASE_EXPECT_VERSION"]
    assert backend["core"] == "rust"
    assert backend["build_commit"] == expected_commit
    assert backend["build_dirty"] is None or type(backend["build_dirty"]) is bool
    version = importlib.metadata.version("llmwiki")
    assert version == expected_version
    return backend, version


def run_tests(tests: Path, support_scripts: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="llmbase-wheel-tests-") as temporary:
        harness = Path(temporary)
        copied_tests = harness / "tests"
        shutil.copytree(tests, copied_tests)
        shutil.copytree(support_scripts, harness / "scripts" / "rust-migration")
        public_projection = copied_tests / "rust_migration" / "test_http_public_projections.py"
        common = [sys.executable, "-m", "pytest", "--import-mode=importlib", "--timeout=60", "-q"]
        subprocess.run([*common, str(public_projection)], cwd=harness, check=True)
        subprocess.run(
            [*common, str(copied_tests), f"--ignore={public_projection}"],
            cwd=harness,
            check=True,
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--tests", type=Path)
    parser.add_argument("--support-scripts", type=Path)
    parser.add_argument("--wheel", type=Path)
    parser.add_argument("--platform")
    parser.add_argument("--receipt", type=Path)
    args = parser.parse_args()
    if (args.tests is None) != (args.support_scripts is None):
        parser.error("--tests and --support-scripts must be supplied together")
    if args.receipt is not None and (args.wheel is None or args.platform is None):
        parser.error("--receipt requires --wheel and --platform")

    for name in SENSITIVE_ENV:
        os.environ.pop(name, None)
    os.environ["LLMBASE_REQUIRE_NATIVE"] = "1"
    backend, version = installed_backend()
    if args.tests is not None:
        run_tests(args.tests.resolve(), args.support_scripts.resolve())

    if args.wheel is not None:
        wheel = args.wheel.resolve()
        assert "cp311-abi3" in wheel.name, wheel.name
        if args.platform == "manylinux_2_28_x86_64":
            assert "manylinux_2_28_x86_64" in wheel.name, wheel.name
        elif args.platform == "macosx_11_0_arm64":
            assert "macosx_11_0_arm64" in wheel.name, wheel.name
        else:
            raise AssertionError(f"unexpected release platform: {args.platform}")
        receipt = {
            "artifact": wheel.name,
            "sha256": hashlib.sha256(wheel.read_bytes()).hexdigest(),
            "version": version,
            "build_commit": backend["build_commit"],
            "build_dirty": backend["build_dirty"],
            "platform": args.platform,
            "python": sys.version,
        }
        if args.receipt is not None:
            args.receipt.parent.mkdir(parents=True, exist_ok=True)
            args.receipt.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
        print(json.dumps(receipt, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
