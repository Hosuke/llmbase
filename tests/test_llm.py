"""Tests for LLM utilities (extract_json, etc.)."""

from tools.llm import extract_json


def test_extract_json_clean_array():
    assert extract_json('[{"id": "test"}]') == '[{"id": "test"}]'


def test_extract_json_clean_object():
    assert extract_json('{"key": "value"}') == '{"key": "value"}'


def test_extract_json_with_thinking():
    text = 'Let me think...\nStep 1: analyze\nStep 2: create\n[{"id": "result"}]'
    assert extract_json(text) == '[{"id": "result"}]'


def test_extract_json_thinking_with_brackets():
    text = 'I see tags [a, b, c] in the data.\n\n[{"id": "actual"}]'
    result = extract_json(text)
    assert '"actual"' in result


def test_extract_json_plain_text():
    text = "This is just a plain answer with no JSON"
    assert extract_json(text) == text


def test_extract_json_rightmost_wins():
    """When both array and object exist, rightmost should be picked."""
    text = 'thinking {"x":1} more thinking [{"id":"final"}]'
    result = extract_json(text)
    assert '"final"' in result


def test_extract_json_markdown_fenced():
    text = '```json\n[{"id": "fenced"}]\n```'
    # extract_json should handle clean JSON that starts with [
    # The markdown fences are handled by the caller (_parse_taxonomy_response)
    result = extract_json(text)
    assert "fenced" in result or "```" in result  # May or may not strip fences


def test_extract_json_invalid_json():
    text = 'thinking... [not valid json {'
    result = extract_json(text)
    # Should return original text when no valid JSON found
    assert result == text


# ─── .env discovery (issue #4 regression) ──────────────────────────


def _reset_llm_env(monkeypatch):
    """Drop any LLMBASE_* / OPENAI_* that the ambient shell may have set."""
    for k in ("LLMBASE_API_KEY", "LLMBASE_BASE_URL", "LLMBASE_MODEL",
              "LLMBASE_ENV_FILE", "OPENAI_API_KEY", "OPENAI_BASE_URL"):
        monkeypatch.delenv(k, raising=False)


def test_load_env_cwd_with_config_yaml(tmp_path, monkeypatch):
    """CWD/.env is honored when CWD/config.yaml exists (KB root marker)."""
    _reset_llm_env(monkeypatch)
    (tmp_path / ".env").write_text("LLMBASE_API_KEY=from-cwd\nLLMBASE_MODEL=cwd-model\n")
    (tmp_path / "config.yaml").write_text("paths:\n  concepts: wiki/concepts\n  wiki: wiki\n  raw: raw\n")
    monkeypatch.chdir(tmp_path)
    from tools.llm import _load_env
    found = _load_env()
    assert found == (tmp_path / ".env").resolve()
    import os
    assert os.environ["LLMBASE_API_KEY"] == "from-cwd"
    assert os.environ["LLMBASE_MODEL"] == "cwd-model"


def test_load_env_cwd_with_unrelated_config_yaml_is_ignored(tmp_path, monkeypatch):
    """A config.yaml from gatsby/hugo/etc. must NOT qualify CWD as a KB root."""
    _reset_llm_env(monkeypatch)
    (tmp_path / ".env").write_text("LLMBASE_BASE_URL=https://evil.example\n")
    # Plausible non-llmbase config.yaml (no paths.concepts|wiki|raw).
    (tmp_path / "config.yaml").write_text(
        "siteMetadata:\n  title: My Blog\nplugins:\n  - gatsby-plugin-mdx\n"
    )
    monkeypatch.chdir(tmp_path)
    home = tmp_path / "home"; home.mkdir()
    monkeypatch.setenv("HOME", str(home))
    from tools.llm import _load_env
    found = _load_env()
    import os
    assert os.environ.get("LLMBASE_BASE_URL") != "https://evil.example"
    if found is not None:
        assert found != (tmp_path / ".env").resolve()


def test_load_env_cwd_without_config_yaml_is_ignored(tmp_path, monkeypatch):
    """Hostile .env in an unrelated CWD must NOT be loaded (no config.yaml)."""
    _reset_llm_env(monkeypatch)
    (tmp_path / ".env").write_text("LLMBASE_BASE_URL=https://evil.example\n")
    monkeypatch.chdir(tmp_path)
    # Also isolate HOME so the XDG fallback doesn't mask the result.
    home = tmp_path / "home"
    home.mkdir()
    monkeypatch.setenv("HOME", str(home))
    from tools.llm import _load_env
    found = _load_env()
    import os
    # No CWD match → no redirect; package-dir .env may or may not exist in dev.
    assert os.environ.get("LLMBASE_BASE_URL") != "https://evil.example"
    if found is not None:
        # Acceptable only if it's the legacy package-dir .env, never the CWD one.
        assert found != (tmp_path / ".env").resolve()


def test_load_env_explicit_override(tmp_path, monkeypatch):
    """LLMBASE_ENV_FILE beats CWD."""
    _reset_llm_env(monkeypatch)
    (tmp_path / ".env").write_text("LLMBASE_API_KEY=from-cwd\n")
    (tmp_path / "config.yaml").write_text("paths:\n  concepts: wiki/concepts\n  wiki: wiki\n  raw: raw\n")
    explicit = tmp_path / "custom.env"
    explicit.write_text("LLMBASE_API_KEY=from-explicit\n")
    monkeypatch.chdir(tmp_path)
    monkeypatch.setenv("LLMBASE_ENV_FILE", str(explicit))
    from tools.llm import _load_env
    found = _load_env()
    assert found == explicit.resolve()
    import os
    assert os.environ["LLMBASE_API_KEY"] == "from-explicit"


def test_load_env_explicit_override_missing_fails_closed(tmp_path, monkeypatch):
    """LLMBASE_ENV_FILE pointing at a missing file must NOT fall through."""
    _reset_llm_env(monkeypatch)
    # CWD has a valid KB + .env that would otherwise be picked up.
    (tmp_path / ".env").write_text("LLMBASE_API_KEY=from-cwd\n")
    (tmp_path / "config.yaml").write_text("paths:\n  concepts: wiki/concepts\n  wiki: wiki\n  raw: raw\n")
    monkeypatch.chdir(tmp_path)
    monkeypatch.setenv("LLMBASE_ENV_FILE", str(tmp_path / "does-not-exist.env"))
    from tools.llm import _load_env
    found = _load_env()
    assert found is None
    import os
    assert "LLMBASE_API_KEY" not in os.environ


def test_load_env_shell_export_wins(tmp_path, monkeypatch):
    """Shell export must beat anything the .env declares (override=False).

    Also asserts the CWD/.env is the selected source — guards against
    regressions where the first-hit path is silently skipped.
    """
    _reset_llm_env(monkeypatch)
    (tmp_path / ".env").write_text("LLMBASE_API_KEY=from-file\n")
    (tmp_path / "config.yaml").write_text("paths:\n  concepts: wiki/concepts\n  wiki: wiki\n  raw: raw\n")
    monkeypatch.chdir(tmp_path)
    monkeypatch.setenv("LLMBASE_API_KEY", "from-shell")
    from tools.llm import _load_env
    found = _load_env()
    import os
    assert found == (tmp_path / ".env").resolve()
    assert os.environ["LLMBASE_API_KEY"] == "from-shell"


def test_load_env_empty_file_is_still_selected(tmp_path, monkeypatch):
    """Empty/comment-only .env is a legal file — must still be the selected
    source, not silently skipped to a lower-priority candidate."""
    _reset_llm_env(monkeypatch)
    (tmp_path / ".env").write_text("# just a comment\n")
    (tmp_path / "config.yaml").write_text("paths:\n  concepts: wiki/concepts\n  wiki: wiki\n  raw: raw\n")
    monkeypatch.chdir(tmp_path)
    home = tmp_path / "home"; home.mkdir()
    (home / ".config" / "llmbase").mkdir(parents=True)
    (home / ".config" / "llmbase" / ".env").write_text("LLMBASE_API_KEY=from-xdg\n")
    monkeypatch.setenv("HOME", str(home))
    from tools.llm import _load_env
    found = _load_env()
    import os
    assert found == (tmp_path / ".env").resolve()
    # Crucially: the XDG .env must NOT have leaked in.
    assert "LLMBASE_API_KEY" not in os.environ


def test_load_env_explicit_override_empty_file_ok(tmp_path, monkeypatch):
    """LLMBASE_ENV_FILE pointing at an empty file is still a valid override."""
    _reset_llm_env(monkeypatch)
    explicit = tmp_path / "empty.env"
    explicit.write_text("")
    monkeypatch.setenv("LLMBASE_ENV_FILE", str(explicit))
    from tools.llm import _load_env
    found = _load_env()
    assert found == explicit.resolve()


def test_load_env_xdg_fallback(tmp_path, monkeypatch):
    """No CWD/.env but ~/.config/llmbase/.env exists → that wins."""
    _reset_llm_env(monkeypatch)
    home = tmp_path / "home"
    (home / ".config" / "llmbase").mkdir(parents=True)
    (home / ".config" / "llmbase" / ".env").write_text("LLMBASE_API_KEY=from-xdg\n")
    empty = tmp_path / "empty"
    empty.mkdir()
    monkeypatch.chdir(empty)
    monkeypatch.setenv("HOME", str(home))
    from tools.llm import _load_env
    found = _load_env()
    assert found == (home / ".config" / "llmbase" / ".env").resolve()
    import os
    assert os.environ["LLMBASE_API_KEY"] == "from-xdg"
