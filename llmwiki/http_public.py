"""Explicit public projections for the optional, trusted Flask host adapter.

Registration attests to the host's entire reviewed filtering pipeline. It is not
a sandbox or automatic proof that arbitrary Python code preserves privacy.
"""
from dataclasses import dataclass

from flask import Flask, jsonify, request

from .http_auth import authorize, private_response_headers


_KEY = "llmbase.public_read_projections.v1"
_READ_METHODS = frozenset({"GET", "HEAD", "OPTIONS"})
_ENDPOINTS = frozenset({
    "api_article", "api_article_sections", "api_source_detail", "api_articles",
    "api_articles_lite", "api_sources", "api_entities", "api_stats", "api_search",
    "api_collections", "api_aliases", "api_export_article", "api_export_tag",
    "api_export_graph", "api_taxonomy", "api_xici", "api_trails", "api_health",
    "api_wiki_export",
})


def _hooks(app):
    # Include app and blueprint chains, their order and exact function objects.
    # Retain each object as well as its id: custom callable equality cannot make
    # a replacement compare equal, and retained objects cannot have ids reused.
    return tuple(tuple((scope, tuple((id(hook), hook) for hook in chain))
                       for scope, chain in mapping.items())
                 for mapping in (app.before_request_funcs, app.after_request_funcs))


def _rules(app, endpoint):
    return tuple((rule.rule, rule.subdomain, rule.host, frozenset(rule.methods or ()))
                 for rule in app.url_map.iter_rules(endpoint))


@dataclass(frozen=True)
class _Projection:
    view: object
    hooks: tuple
    rules: tuple


class PublicProjectionFlask(Flask):
    def process_response(self, response):
        # Enforce the binding after mutable host hooks have run: an accidentally
        # replaced after_request hook must not rewrite an invalid binding's 503
        # into an unfiltered 200. This is still a trusted in-process host API.
        return public_read_projection_headers(self, super().process_response(response))


def init_public_read_projections(app, builtin_endpoints):
    app.extensions[_KEY] = {"eligible": _ENDPOINTS.intersection(builtin_endpoints),
                            "registered": {}}


def register_public_read_projection(app, endpoint, view):
    """Install a reviewed public read pipeline before the app's first request.

    Call after all view wrappers and before/after hooks are composed. The host
    owns live source authorization, safe aggregate provenance and filtering of
    every returned field. Registration changes neither principal nor scopes.
    Later replacement of a view, route or hook invalidates this attestation.
    """
    state = app.extensions.get(_KEY)
    if state is None or app._got_first_request:
        raise ValueError("public projections require an unserved llmbase web app")
    if not isinstance(endpoint, str) or endpoint not in state["eligible"]:
        raise ValueError("endpoint is not a supported public read projection")
    if endpoint in state["registered"]:
        raise ValueError("public projection is already registered")
    if not callable(view):
        raise TypeError("public projection view must be callable")
    rules = _rules(app, endpoint)
    if not rules or any(not methods or not methods <= _READ_METHODS
                        for _, _, _, methods in rules):
        raise ValueError("every endpoint route must be read-only")
    if endpoint not in app.view_functions:
        raise ValueError("public projection endpoint has no view")
    projection = _Projection(view, _hooks(app), rules)
    # Validation is complete before either registration or dispatch is changed.
    app.view_functions[endpoint] = view
    state["registered"][endpoint] = projection


def _current_projection(app):
    state = app.extensions.get(_KEY)
    return state["registered"].get(request.endpoint) if state else None


def _binding_valid(app, projection):
    return (app.view_functions.get(request.endpoint) is projection.view
            and _hooks(app) == projection.hooks
            and _rules(app, request.endpoint) == projection.rules
            and request.method in _READ_METHODS)


def _binding_error():
    return jsonify({"status": "error", "message": "public projection binding changed"}), 503


def public_read_projection_gate(app):
    """Return (handled, denial); only a valid explicit binding skips the scan."""
    projection = _current_projection(app)
    if projection is None:
        return False, None
    if not _binding_valid(app, projection):
        return True, _binding_error()
    denied = authorize("read")
    if denied is not None:
        return True, denied
    # A legacy ETag cannot authorize data after a private flag changes. Hosts
    # with this transitional contract always return a fresh filtered response.
    for header in ("HTTP_IF_NONE_MATCH", "HTTP_IF_MODIFIED_SINCE"):
        request.environ.pop(header, None)
    return True, None


def public_read_projection_headers(app, response):
    projection = _current_projection(app)
    if projection is not None:
        if not _binding_valid(app, projection):
            response = app.make_response(_binding_error())
        else:
            denied = authorize("read")
            if denied is not None:
                response = app.make_response(denied)
        if response.status_code == 304:
            response = jsonify({"status": "error", "message": "public projection must return a fresh response"})
            response.status_code = 503
        response.headers["Cache-Control"] = "no-store"
        response.headers.pop("ETag", None)
        response.headers.pop("Last-Modified", None)
        response = private_response_headers(response)
    return response
