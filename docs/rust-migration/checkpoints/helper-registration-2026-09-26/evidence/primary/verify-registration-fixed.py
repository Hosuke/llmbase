"""Independent installed-wheel acceptance of the inferred-registration repair."""
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import sqlite3
import sys
import tempfile

import llmwiki
from llmwiki import _core, _native, atomic, compile, config

prefix = Path(sys.prefix).resolve()
assert Path(llmwiki.__file__).resolve().is_relative_to(prefix)
assert Path(_core.__file__).resolve().is_relative_to(prefix)
with tempfile.TemporaryDirectory(prefix="b5-primary-fixed-") as directory:
    base = Path(directory)
    cfg = config.load_config(base)
    config.ensure_dirs(cfg)
    before = deepcopy(cfg)
    registry = set(_native._configured_roots)
    raw, concepts, meta = (Path(cfg["paths"][name]) for name in ("raw", "concepts", "meta"))
    source = raw / "source" / "index.md"
    source.parent.mkdir()
    source_bytes = "---\ntitle: Source\nunknown: {keep: true}\n---\n原文𠮷\r\n".encode()
    source.write_bytes(source_bytes)
    assert compile._find_uncompiled(raw) == [source]
    concept = concepts / "one.md"
    concept.write_text("---\ntitle: One\n---\nConcept body\n")
    assert compile._list_existing_concepts(concepts) == ["one (One)"]
    index_rows = [{"slug": "one", "title": "One"}]
    (meta / "index.json").write_text(json.dumps(index_rows))
    assert compile._load_index(meta) == index_rows
    assert set(_native._configured_roots) == registry
    assert cfg == before
    assert not list(base.rglob("*.sqlite3")), "read helpers created a database"

    probe = meta / "probe.bin"
    old, new = "原字𠮷\r\n".encode(), "新字é\r\n".encode()
    atomic.atomic_write_text(probe, old.decode())
    atomic.atomic_write_text(probe, new.decode())
    assert probe.read_bytes() == new
    assert source.read_bytes() == source_bytes
    database = meta / "llmbase.sqlite3"
    assert list(base.rglob("*.sqlite3")) == [database]
    with sqlite3.connect(database) as db:
        rows = db.execute("SELECT id,library_id,before_hash,before_bytes,after_hash,after_bytes,status FROM write_journal WHERE relative_path=?", ("probe.bin",)).fetchall()
    digest = lambda value: hashlib.sha256(value).hexdigest()
    expected = {(None, None, digest(old), old), (digest(old), old, digest(new), new)}
    assert len(rows) == 2 and len({row[0] for row in rows}) == 2
    assert {(r[2], r[3], r[4], r[5]) for r in rows} == expected
    assert all(r[1] == "default" and r[6] == "applied" for r in rows)

    other = base / "other"
    other.mkdir()
    _native.register_config({"base_dir": str(other), "paths": dict(cfg["paths"])}, other)
    try:
        atomic.atomic_write_text(probe, "ambiguous")
    except _native.NativeCapabilityError as error:
        assert "multiple configured libraries" in str(error)
    else:
        raise AssertionError("real overlapping libraries were not refused")
    assert probe.read_bytes() == new
    _native.with_writer(base, lambda: atomic.atomic_write_text(probe, "explicit"), paths=cfg["paths"])
    assert probe.read_bytes() == b"explicit"
    assert not list(other.rglob("*.sqlite3"))
    print(json.dumps({"result": "passed", "read_helper_registry_unchanged": True,
        "registered_config_unchanged": True, "raw_bytes_unchanged": True,
        "read_helpers_created_no_database": True, "exact_journal_rows": len(rows),
        "journal_applied": True, "real_overlap_refused": True,
        "explicit_writer_succeeded": True, "package": llmwiki.__file__,
        "extension": _core.__file__, "extension_sha256": digest(Path(_core.__file__).read_bytes())}, indent=2))
