"""Installed native wheels disclose verifiable source provenance."""
import os
import re

import pytest

from llmwiki import _native


def test_native_build_provenance():
    if not _native.available():
        pytest.skip("native extension is not built in this checkout")

    info = _native.backend_info()
    commit = info["build_commit"]
    assert commit == "unknown" or re.fullmatch(r"[0-9a-f]{40}(?:[0-9a-f]{24})?", commit)
    assert info["build_dirty"] is None or type(info["build_dirty"]) is bool

    expected = os.environ.get("LLMBASE_EXPECT_BUILD_COMMIT")
    if expected is not None:
        assert re.fullmatch(r"[0-9a-fA-F]{40}(?:[0-9a-fA-F]{24})?", expected)
        assert commit == expected.lower()
