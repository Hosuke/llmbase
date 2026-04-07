"""Tests for atomic file writes."""

import json
from pathlib import Path

from tools.atomic import atomic_write_json


def test_atomic_write_creates_file(tmp_path):
    path = tmp_path / "test.json"
    data = {"key": "value"}
    atomic_write_json(path, data)

    assert path.exists()
    assert json.loads(path.read_text()) == data


def test_atomic_write_overwrites(tmp_path):
    path = tmp_path / "test.json"
    atomic_write_json(path, {"version": 1})
    atomic_write_json(path, {"version": 2})

    assert json.loads(path.read_text())["version"] == 2


def test_atomic_write_creates_dirs(tmp_path):
    path = tmp_path / "deep" / "nested" / "test.json"
    atomic_write_json(path, {"nested": True})

    assert path.exists()
    assert json.loads(path.read_text())["nested"] is True


def test_atomic_write_unicode(tmp_path):
    path = tmp_path / "unicode.json"
    data = {"name": "孔子", "role": "哲学家"}
    atomic_write_json(path, data)

    loaded = json.loads(path.read_text())
    assert loaded["name"] == "孔子"


def test_atomic_write_no_temp_leftover(tmp_path):
    path = tmp_path / "clean.json"
    atomic_write_json(path, {"clean": True})

    # No .tmp files should remain
    tmp_files = list(tmp_path.glob(".*tmp*"))
    assert len(tmp_files) == 0
