# Rust: verify HTTP contracts and the existing frontend against both site profiles

## Starting point
Native Web/Agent HTTP routes and React integration exist, but current complete UI acceptance is open. Known parity questions include Last-Modified, configuration reload, graph depth and profile-dependent envelopes.

## Deliverable
Document supported HTTP v1 routes and explicitly decide those remaining contracts. Exercise the existing frontend against an isolated native server with English and wenyan profiles. Fix only observed adapter/UI mismatches; keep authorization and domain operations in shared Rust services. Provide a migration recipe for retained Flask custom routes/middleware or their native replacement.

## Acceptance
Automate one real browser flow: open library, search, inspect article/source evidence, stream an answer, observe cancellation/error, and explicitly save/review where supported. Verify distinct libraries, private denial, config/profile refresh, graph depth limits and cache headers. Public read must not grant model or write capability. Source hashes remain unchanged until an explicit authorized write.

## Boundary and dependencies
Depends on 01/02 and relevant service issues. Own HTTP adapters/frontend tests and narrowly required frontend fixes. Run server/browser checks on an isolated runner; no redesign, production deployment or always-on local server.

## Working rule
This issue is saved design, not a completed feature. Implement one bounded change at a time, independently review/accept it, then commit before starting the next. Preserve source bytes, library isolation and recovery guarantees. See [ADR 0002](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/adr/0002-clean-rust-major-and-extensions.md). Dependencies refer to numbered design items in the [handoff](https://github.com/Hosuke/llmbase/blob/rust-core-rebuild/docs/rust-migration/issues/README.md).
