"""Configuration loader."""

import os
from copy import deepcopy
from pathlib import Path

import yaml


def load_config(base_dir: Path | None = None) -> dict:
    """Load config.yaml from the project root."""
    if base_dir is None:
        base_dir = Path.cwd()
    config_path = base_dir / "config.yaml"
    if not config_path.exists():
        cfg = _defaults(base_dir)
        cfg["base_dir"] = str(Path(base_dir).resolve())
        return cfg
    with open(config_path) as f:
        cfg = yaml.safe_load(f)
    # Resolve relative paths against base_dir
    for key in ("raw", "wiki", "outputs", "meta", "concepts"):
        p = cfg.get("paths", {}).get(key)
        if p:
            cfg["paths"][key] = str((base_dir / p).resolve())
    cfg["base_dir"] = str(Path(base_dir).resolve())
    return cfg


def _defaults(base_dir: Path | None = None) -> dict:
    base = Path(base_dir) if base_dir else Path.cwd()
    return {
        "llm": {"model": "claude-sonnet-4-6", "max_tokens": 16384},
        "paths": {
            "raw": str(base / "raw"),
            "wiki": str(base / "wiki"),
            "outputs": str(base / "wiki" / "outputs"),
            "meta": str(base / "wiki" / "_meta"),
            "concepts": str(base / "wiki" / "concepts"),
        },
        "compile": {"batch_size": 10, "backlinks": True},
        "search": {"port": 5555},
        "lint": {"web_search": False},
        "worker": {
            "enabled": False,
            "learn_interval_hours": 6,
            "compile_interval_hours": 1,
            "taxonomy_interval_hours": 12,
            "health_check_interval_hours": 24,
            "learn_batch_size": 10,
            "learn_source": "cbeta",
        },
        "health": {
            "auto_fix_broken_links": True,
            "max_stubs_per_run": 10,
        },
        "entities": {
            "enabled": False,
            "extract_interval_hours": 24,
        },
        "languages": deepcopy(BUILTIN_LANGUAGES),
    }


def ensure_dirs(cfg: dict):
    """Create all configured directories if they don't exist."""
    for key in ("raw", "wiki", "outputs", "meta", "concepts"):
        Path(cfg["paths"][key]).mkdir(parents=True, exist_ok=True)


# ─── Language profile contract ───────────────────────────────────
# Single source of truth for "what languages does this KB speak".
# Resolution order (get_language_profile): import-time override of
# compile.SECTION_HEADERS  >  config.yaml `languages:`  >  builtin default.
BUILTIN_LANGUAGES: dict = {
    "default": "trilingual",
    "profiles": {
        "trilingual": {
            "default_lang": "zh-en",
            "api_default_lang": "zh",
            "sections": [
                {"code": "en", "key": "english", "header": "## English",
                 "label": "English", "icon": "EN", "title_hint": "latin"},
                {"code": "zh", "key": "中文", "header": "## 中文",
                 "label": "中文", "icon": "中", "title_hint": "cjk"},
                {"code": "ja", "key": "日本語", "header": "## 日本語",
                 "label": "日本語", "icon": "日", "title_hint": "cjk"},
            ],
            "views": [
                {"code": "zh-en", "show": ["en", "zh"], "label": "中英双语",
                 "icon": "双", "title_hint": "full", "primary": "zh"},
            ],
        },
    },
}


def get_language_profile(cfg: dict | None = None) -> dict:
    """Return the normalized language profile for this knowledge base."""
    from . import compile as _compile_mod

    if _compile_mod.SECTION_HEADERS is not _compile_mod.DEFAULT_SECTION_HEADERS:
        return _profile_from_section_headers(_compile_mod.SECTION_HEADERS)

    if cfg and cfg.get("languages"):
        return _normalize_languages(cfg["languages"])

    return _normalize_languages(BUILTIN_LANGUAGES)


def get_section_headers(cfg: dict | None = None) -> list[tuple[str, str]]:
    """Return compile-style section headers from the active language profile."""
    return [(s["key"], s["header"]) for s in get_language_profile(cfg)["sections"]]


def _profile_from_section_headers(section_headers: list[tuple[str, str]]) -> dict:
    if not section_headers:
        raise ValueError("compile.SECTION_HEADERS override must contain at least one section")

    sections = []
    for key, header in section_headers:
        label = header.lstrip("#").strip() or key
        icon = label[:1] or key[:1]
        sections.append({
            "code": key,
            "key": key,
            "header": header,
            "label": label,
            "icon": icon,
            "title_hint": "full",
        })

    codes = [s["code"] for s in sections]
    return {
        "name": "_patched",
        "sections": sections,
        "views": [],
        # Legacy pre-contract deployments patch SECTION_HEADERS at import time;
        # web.py _resolve_lang historically defaulted every lang surface to "zh".
        # Declare that same default so new frontends pick it up without
        # inventing a lang.
        "default_lang": "zh",
        "api_default_lang": "zh",
        "codes": codes,
        "single_section": len(sections) == 1,
        "bare_body": len(sections) == 1 and sections[0]["header"] == "",
    }


def _normalize_languages(languages: dict) -> dict:
    default_name = languages.get("default")
    profiles = languages.get("profiles") or {}
    if default_name not in profiles:
        raise ValueError("languages.default must name an existing profile")
    return _normalize_profile(default_name, profiles[default_name])


def _normalize_profile(name: str, profile: dict) -> dict:
    raw_sections = profile.get("sections") or []
    if not raw_sections:
        raise ValueError(f"languages.profiles.{name}.sections must contain at least one section")

    sections = []
    section_codes = set()
    for i, raw in enumerate(raw_sections):
        key = raw.get("key")
        if not key:
            raise ValueError(f"languages.profiles.{name}.sections[{i}].key must be non-empty")
        header = raw.get("header")
        if not isinstance(header, str):
            raise ValueError(f"languages.profiles.{name}.sections[{i}].header must be a string")
        code = raw.get("code", key)
        label = raw.get("label", key)
        icon = raw.get("icon", label[:1])
        sections.append({
            "code": code,
            "key": key,
            "header": header,
            "label": label,
            "icon": icon,
            "title_hint": raw.get("title_hint", "full"),
        })
        section_codes.add(code)

    if len(raw_sections) > 1 and any(s["header"] == "" for s in sections):
        raise ValueError("empty section header is only allowed in single-section profiles")

    views = []
    codes = []
    seen_codes = set()
    for section in sections:
        code = section["code"]
        if code in seen_codes:
            raise ValueError(f"duplicate language code: {code}")
        seen_codes.add(code)
        codes.append(code)

    for i, raw in enumerate(profile.get("views") or []):
        code = raw.get("code")
        if not code:
            raise ValueError(f"languages.profiles.{name}.views[{i}].code must be non-empty")
        if code in seen_codes:
            raise ValueError(f"duplicate language code: {code}")
        show = raw.get("show")
        if not show or any(c not in section_codes for c in show):
            raise ValueError(
                f"languages.profiles.{name}.views[{i}].show must be a non-empty subset of section codes"
            )
        primary = raw.get("primary", show[0])
        if primary not in section_codes:
            raise ValueError(f"languages.profiles.{name}.views[{i}].primary must be a section code")
        label = raw.get("label", code)
        views.append({
            "code": code,
            "show": list(show),
            "label": label,
            "icon": raw.get("icon", label[:1]),
            "title_hint": raw.get("title_hint", "full"),
            "primary": primary,
        })
        seen_codes.add(code)
        codes.append(code)

    default_lang = profile.get("default_lang", sections[0]["code"])
    api_default_lang = profile.get("api_default_lang", sections[0]["code"])
    if default_lang not in seen_codes:
        raise ValueError(f"languages.profiles.{name}.default_lang must be one of the profile codes")
    if api_default_lang not in seen_codes:
        raise ValueError(f"languages.profiles.{name}.api_default_lang must be one of the profile codes")

    single_section = len(sections) == 1
    return {
        "name": name,
        "sections": sections,
        "views": views,
        "default_lang": default_lang,
        "api_default_lang": api_default_lang,
        "codes": codes,
        "single_section": single_section,
        "bare_body": single_section and sections[0]["header"] == "",
    }
