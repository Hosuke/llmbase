"""Explicit principals for the optional Flask transports.

Visitor cookies are never credentials. Runtime token_scopes is an in-memory
mapping for embedding hosts; no bearer value is returned in a principal or log.
"""
import hmac
import ipaddress
import os

from flask import current_app, g, jsonify, request

SCOPES = frozenset({"read", "private", "model", "write", "operator"})


def runtime_config(base, cfg, *, api_secret="", session_token=""):
    return {"base_dir": base, "cfg": cfg, "api_secret": api_secret,
            "session_token": session_token, "token_scopes": {},
            "allow_unauthenticated_local": os.getenv("LLMBASE_ALLOW_LOCAL_UNAUTHENTICATED", "").lower() in {"1", "true", "yes"}}


def principal():
    from .web import _parse_bearer
    config = current_app.config.get("llmbase", {})
    credential = _parse_bearer(request.headers.get("Authorization", ""))
    secret = config.get("api_secret", "")
    scopes, kind = {"read"}, "visitor"
    if credential and secret and hmac.compare_digest(credential.encode(), secret.encode()):
        scopes, kind = set(SCOPES), "operator"
    elif credential:
        for token, granted in config.get("token_scopes", {}).items():
            if isinstance(token, str) and token and hmac.compare_digest(credential.encode(), token.encode()):
                if isinstance(granted, (list, tuple, set, frozenset)) and set(granted) <= SCOPES:
                    scopes, kind = set(granted), "bearer"
                break
    elif not secret and config.get("allow_unauthenticated_local") is True:
        # Explicit local development only; forwarded requests are never local
        # merely because a reverse proxy connects over a loopback socket.
        try:
            local = ipaddress.ip_address(request.remote_addr or "").is_loopback
        except ValueError:
            local = False
        if local and not any(h in request.headers for h in ("Forwarded", "X-Forwarded-For", "X-Real-IP")):
            scopes, kind = set(SCOPES), "local"
    result = {"kind": kind, "scopes": scopes}
    g.llmbase_principal = {"kind": kind, "scopes": sorted(scopes)}
    return result


def authorize(*required, context=None):
    current = principal()
    if set(required) <= current["scopes"]:
        return None
    status = 401 if current["kind"] == "visitor" else 403
    message = "Authorization: Bearer credentials with required scopes needed"
    if context:
        message = context + " requires " + message
    return jsonify({"status": "error", "message": message,
                    "required_scopes": sorted(set(required))}), status


def can_read_private():
    return "private" in principal()["scopes"]


def operation_scopes(op, arguments):
    from . import operations
    configured = getattr(op, "required_scopes", None)
    if configured is not None:
        if not isinstance(configured, (tuple, list, set, frozenset)) or not set(configured) <= SCOPES:
            return ("operator",)
        required = set(configured)
    elif any(op is known for known in operations._CANONICAL):
        required = {"read"}
        if op.name == "kb_ask" or op.name in {"kb_compile", "kb_lint_fix"} or (
            op.name == "kb_lint" and (arguments.get("deep") or arguments.get("fix"))):
            required.add("model")
    else:
        # category='read' and writes=False do not prove a dynamic handler is safe.
        return ("operator",)
    if operations._needs_write_lock(op, arguments):
        required.add("write")
    if op.name == "kb_ingest" and not str(arguments.get("source", "")).startswith(("http://", "https://")):
        # Local file ingestion can read server files; a generic remote write
        # grant must not become arbitrary filesystem read authority.
        required.add("operator")
    return tuple(sorted(required))


def private_metadata(metadata):
    # Keep Python detail reads aligned with Rust visibility::is_public. Equality
    # against False would also accept 0; malformed flags must fail closed.
    flag = metadata.get("private")
    return (flag is not None and flag is not False) or metadata.get("visibility", "public") != "public"


def public_search_hit(snapshot, cfg, row, *, raw=False):
    from pathlib import Path
    if raw:
        relative = row.get("rel_path", "")
        slug = relative[:-3] if relative.endswith(".md") else relative
        if slug.endswith("/index"):
            slug = slug[:-6]
        return snapshot["raw"].get(slug, {}).get("public", False)
    if row.get("path"):
        path = Path(row["path"]).resolve()
        for kind, key in (("compiled", "concepts"), ("note", "outputs")):
            root = Path(cfg["paths"][key]).resolve()
            if path.is_relative_to(root):
                slug = path.relative_to(root).with_suffix("").as_posix()
                return snapshot[kind].get(slug, {}).get("public", False)
        return False
    return snapshot["compiled"].get(row.get("slug", ""), {}).get("public", False)


def visibility_snapshot_error(error):
    """Expose actionable capacity diagnostics without source paths or metadata."""
    code = getattr(error, "code", None)
    if code == "visibility_snapshot_limit_exceeded":
        resource = getattr(error, "resource", None)
        limit, observed = getattr(error, "limit", None), getattr(error, "observed", None)
        if (resource in {"max_files", "max_read_bytes", "max_header_bytes"}
                and type(limit) is int and type(observed) is int and 0 < limit < observed):
            return {"status": "error", "code": code,
                    "message": f"Public visibility snapshot exceeds configured {resource} limit",
                    "limit": {"resource": resource, "configured": limit, "observed": observed},
                    "config_key": f"visibility_snapshot.{resource}"}
    if code == "visibility_snapshot_config_invalid":
        return {"status": "error", "code": code,
                "message": "Invalid visibility_snapshot configuration; check its documented positive integer limits",
                "config_key": "visibility_snapshot"}
    return {"status": "error", "message": "public visibility snapshot unavailable"}


def visibility_snapshot(base, cfg):
    from . import _native
    if _native.available():
        return _native.dispatch("compat_kb_visibility", {}, base, paths=cfg["paths"])["result"]
    # Explicit unbuilt source checkout only. An installed native candidate cannot
    # enter this fallback; read failures are not treated as public metadata.
    import frontmatter
    from pathlib import Path
    result = {"compiled": {}, "raw": {}, "note": {}, "has_nonpublic": False}
    for kind, name in (("compiled", "concepts"), ("raw", "raw"), ("note", "outputs")):
        root = Path(cfg["paths"][name]).resolve()
        for path in root.rglob("*.md"):
            if ".original" in path.relative_to(root).parts:
                continue
            if path.is_symlink() or not path.resolve().is_relative_to(root):
                raise ValueError("source path must remain within its configured root")
            relative = path.relative_to(root).as_posix()
            slug = relative[:-3]
            if kind == "raw" and slug.endswith("/index"):
                slug = slug[:-6]
            try:
                public = not private_metadata(frontmatter.load(path).metadata)
            except (ValueError, OSError):
                public = False
            result[kind][slug] = {"public": public}
            result["has_nonpublic"] |= not public
    return result


def private_response_headers(response):
    if principal()["kind"] != "visitor":
        response.headers["Cache-Control"] = "private, no-store"
        response.vary.add("Authorization")
        response.vary.add("Cookie")
    return response
