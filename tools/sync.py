"""Remote progress sync — generic PostgREST adapter.

Lets a deployment back its ingestion + compile state in a remote Postgres table
so that an ephemeral filesystem (volume reset, container rebuild, etc.) does
not cause re-ingest of already-known sources. Works against any PostgREST
endpoint (Supabase, PostgREST self-hosted, …).

Configuration is via environment variables — completely no-op when unset:
- LLMBASE_SYNC_URL    — base URL of the PostgREST endpoint
                        (also accepts SUPABASE_URL for backcompat)
- LLMBASE_SYNC_KEY    — bearer token / API key
                        (also accepts SUPABASE_KEY for backcompat)
- LLMBASE_SYNC_TABLE  — table name (default: llmbase_ingested)

Expected table schema:
    source        text         not null   -- arbitrary plugin id, e.g. 'cbeta'
    work_id       text         not null   -- canonical work id within the source
    title         text         null
    ingested_at   timestamptz  default now()
    compiled      bool         default false
    compiled_at   timestamptz  null
    primary key (source, work_id)

This module talks to PostgREST directly via `requests` — no vendor SDK,
keeps llmbase install lean and provider-agnostic.
"""

from __future__ import annotations

import logging
import os
from datetime import datetime, timezone
from typing import Iterable

import requests

logger = logging.getLogger("llmbase.sync")

_TIMEOUT = 8  # seconds — never block worker for long
_DEFAULT_TABLE = "llmbase_ingested"


def _env(*names: str, default: str = "") -> str:
    """First non-empty env var from a fallback chain."""
    for name in names:
        v = os.getenv(name)
        if v:
            return v
    return default


def is_enabled() -> bool:
    """Return True iff sync env vars are set."""
    return bool(
        _env("LLMBASE_SYNC_URL", "SUPABASE_URL")
        and _env("LLMBASE_SYNC_KEY", "SUPABASE_KEY")
    )


def _config() -> tuple[str, dict, str] | None:
    """Return (base_url, headers, table) tuple, or None if not enabled."""
    url = _env("LLMBASE_SYNC_URL", "SUPABASE_URL").rstrip("/")
    key = _env("LLMBASE_SYNC_KEY", "SUPABASE_KEY")
    if not url or not key:
        return None
    table = _env("LLMBASE_SYNC_TABLE", "LLMBASE_REMOTE_TABLE", default=_DEFAULT_TABLE)
    headers = {
        "apikey": key,
        "Authorization": f"Bearer {key}",
        "Content-Type": "application/json",
    }
    return url, headers, table


def pull_ingested(source: str) -> set[str]:
    """Fetch the set of work_ids already ingested for a given source.

    Returns an empty set on any failure (network, auth, etc.) so callers
    can safely union with their local progress without ever blocking learn.
    """
    cfg = _config()
    if cfg is None:
        return set()
    base_url, headers, table = cfg
    try:
        # Paginate in batches of 1000 (PostgREST default page size)
        result: set[str] = set()
        offset = 0
        page_size = 1000
        while True:
            range_headers = dict(headers)
            range_headers["Range-Unit"] = "items"
            range_headers["Range"] = f"{offset}-{offset + page_size - 1}"
            resp = requests.get(
                f"{base_url}/rest/v1/{table}",
                params={"source": f"eq.{source}", "select": "work_id"},
                headers=range_headers,
                timeout=_TIMEOUT,
            )
            if resp.status_code not in (200, 206):
                logger.warning(f"[sync] pull_ingested({source}) → HTTP {resp.status_code}")
                return set()
            page = resp.json()
            if not page:
                break
            result.update(e["work_id"] for e in page if e.get("work_id"))
            if len(page) < page_size:
                break
            offset += page_size
        return result
    except Exception as e:
        logger.warning(f"[sync] pull_ingested({source}) failed: {e}")
        return set()


def push_ingested(source: str, work_id: str, title: str = "") -> bool:
    """Upsert a single (source, work_id) row marking it as ingested.

    Idempotent. Safe to call repeatedly. Returns True on success.
    """
    cfg = _config()
    if cfg is None:
        return False
    base_url, headers, table = cfg
    try:
        upsert_headers = dict(headers)
        upsert_headers["Prefer"] = "resolution=merge-duplicates,return=minimal"
        body = {
            "source": source,
            "work_id": work_id,
            "title": title or None,
            "ingested_at": datetime.now(timezone.utc).isoformat(),
        }
        resp = requests.post(
            f"{base_url}/rest/v1/{table}",
            json=body,
            headers=upsert_headers,
            timeout=_TIMEOUT,
        )
        if resp.status_code not in (200, 201, 204):
            logger.warning(
                f"[sync] push_ingested({source},{work_id}) → HTTP {resp.status_code}: {resp.text[:200]}"
            )
            return False
        return True
    except Exception as e:
        logger.warning(f"[sync] push_ingested({source},{work_id}) failed: {e}")
        return False


def push_ingested_batch(rows: Iterable[dict]) -> int:
    """Bulk upsert. Each row must have at least 'source' and 'work_id'.

    Returns number of rows submitted (not number actually inserted/updated,
    since PostgREST 'minimal' return mode does not surface that).
    """
    cfg = _config()
    if cfg is None:
        return 0
    base_url, headers, table = cfg
    rows = [r for r in rows if r.get("source") and r.get("work_id")]
    if not rows:
        return 0
    try:
        upsert_headers = dict(headers)
        upsert_headers["Prefer"] = "resolution=merge-duplicates,return=minimal"
        now_iso = datetime.now(timezone.utc).isoformat()
        body = [
            {
                "source": r["source"],
                "work_id": r["work_id"],
                "title": r.get("title") or None,
                "ingested_at": r.get("ingested_at") or now_iso,
            }
            for r in rows
        ]
        resp = requests.post(
            f"{base_url}/rest/v1/{table}",
            json=body,
            headers=upsert_headers,
            timeout=_TIMEOUT * 2,
        )
        if resp.status_code not in (200, 201, 204):
            logger.warning(
                f"[sync] push_ingested_batch({len(rows)}) → HTTP {resp.status_code}: {resp.text[:200]}"
            )
            return 0
        return len(rows)
    except Exception as e:
        logger.warning(f"[sync] push_ingested_batch failed: {e}")
        return 0


def mark_compiled(source: str, work_id: str) -> bool:
    """Flip compiled=true / compiled_at=now for a known (source, work_id).

    Uses upsert so it also creates the row if missing — that way a
    compile-only flow without prior ingest tracking still records state.
    Returns True on success.
    """
    cfg = _config()
    if cfg is None:
        return False
    base_url, headers, table = cfg
    try:
        upsert_headers = dict(headers)
        upsert_headers["Prefer"] = "resolution=merge-duplicates,return=minimal"
        now_iso = datetime.now(timezone.utc).isoformat()
        body = {
            "source": source,
            "work_id": work_id,
            "compiled": True,
            "compiled_at": now_iso,
        }
        resp = requests.post(
            f"{base_url}/rest/v1/{table}",
            json=body,
            headers=upsert_headers,
            timeout=_TIMEOUT,
        )
        if resp.status_code not in (200, 201, 204):
            logger.warning(
                f"[sync] mark_compiled({source},{work_id}) → HTTP {resp.status_code}"
            )
            return False
        return True
    except Exception as e:
        logger.warning(f"[sync] mark_compiled({source},{work_id}) failed: {e}")
        return False


def pull_compiled(source: str) -> set[str]:
    """Fetch the set of work_ids already compiled for a given source."""
    cfg = _config()
    if cfg is None:
        return set()
    base_url, headers, table = cfg
    try:
        result: set[str] = set()
        offset = 0
        page_size = 1000
        while True:
            range_headers = dict(headers)
            range_headers["Range-Unit"] = "items"
            range_headers["Range"] = f"{offset}-{offset + page_size - 1}"
            resp = requests.get(
                f"{base_url}/rest/v1/{table}",
                params={
                    "source": f"eq.{source}",
                    "compiled": "eq.true",
                    "select": "work_id",
                },
                headers=range_headers,
                timeout=_TIMEOUT,
            )
            if resp.status_code not in (200, 206):
                logger.warning(f"[sync] pull_compiled({source}) → HTTP {resp.status_code}")
                return set()
            page = resp.json()
            if not page:
                break
            result.update(e["work_id"] for e in page if e.get("work_id"))
            if len(page) < page_size:
                break
            offset += page_size
        return result
    except Exception as e:
        logger.warning(f"[sync] pull_compiled({source}) failed: {e}")
        return set()
