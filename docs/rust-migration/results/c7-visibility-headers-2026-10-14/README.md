# C7 — bounded native visibility headers, 2026-10-14

Fixes [#26](https://github.com/Hosuke/llmbase/issues/26): a library's Markdown
body size no longer causes the Flask visibility gate to reject public reads.
The snapshot reads framed metadata, retaining the existing privacy predicate,
symlink rejection and shared writer-lock behavior. It does not create an index.

The reader preserves `Document::parse` framing and parsing. It limits each header
to 1 MiB and the snapshot to 20,000 files / 64 MiB of actual reads, including
malformed inputs and buffered read-ahead. The buffer is 1 KiB; headerless files
need a 4–5-byte probe. A bounded extra byte detects exhaustion. Invalid,
unterminated or oversized headers remain non-public. Body UTF-8 validation and
full-document limits remain the responsibility of content readers.

Snapshot `metadata_hash` replaces the old whole-body `content_hash`. Lite ETags
still notice same-size/mtime metadata and privacy changes, without hashing a
body that is absent from the lite response. This does not add cross-request
caching, change the core `private`/`visibility` predicate, or add `public: false`
to that predicate. Downstream public projections retain their own policy.

## Acceptance

- `cargo fmt`, workspace Clippy with `-D warnings`, **365 native tests** and
  standalone release build passed. The subprocess test's repeated result is
  not counted a second time. See [native.log](native.log).
- A fresh installed Linux ABI3 wheel passed **67 focused tests**, then
  **727 full Python tests / 18 subtests**. See [focused.log](focused.log) and
  [full-engine.log](full-engine.log).
- New coverage: seven Rust reader tests, one native snapshot integration test,
  and eight installed-native HTTP cases. These cover >64 MiB cumulative and
  individual bodies, exact framing/EOF, malformed/oversized headers, bounded
  long lines, actual read-ahead accounting, and same-stat privacy/ETag changes.
- All 49 installed Python files match source; native code was rebuilt and the
  wheel installed into a new environment. [Provenance](provenance.json) records
  wheel SHA-256 `331bd0ad93818eadbeaaf58a60aec3df34dbd14cdd00d3c0dd46ceb947ee0bdb`.
  [305 build/source/test input hashes](primary/source-inputs.json) match before
  and after acceptance. Frontend TypeScript and installed lint import passed.
- Independent staged review found a read-ahead accounting gap in the first
  draft. The final reader charges underlying reads and adds a contiguous-input
  regression. The corrected code received LGTM before commit. Earlier build
  artifacts remain under remote `first-attempt/`, outside these final receipts.

## Corpus comparison and its limits

The available remote Taixu tree was **580 Markdown files / 40,723,744 bytes**,
not the reporter's latest 899-file / 83,391,106-byte corpus. Only Markdown/JSON
were copied on the remote host; a separate credential-free config was created.
[The copy receipt](corpus-copy.json) verifies source stability and exact copied
file contents. No production file was written.

Both wheels return identical public response bytes on this real copy. A second
isolated copy extends only one raw body to bring total Markdown to exactly
**83,391,106 bytes**, while each individual file stays below 64 MiB. This is
explicitly **synthetic growth of a real corpus**, not the latest site corpus.
Headers, file count and cached public aggregates remain unchanged.

| Anonymous GET | `5cd2177`, grown copy | C7, grown copy |
| --- | --- | --- |
| `/api/trails` | 503 | 200 |
| `/api/taxonomy?lang=zh` | 503 | 200 |
| `/api/xici?lang=zh` | 503 | 200 |

All three C7 response bodies also match the old wheel's successful responses on
the smaller real copy. Every probe preserves full Markdown/JSON/config hashes.
See [comparison.json](comparison.json), [growth.json](growth.json), and the
[old](baseline-large.json) / [new](candidate-large.json) grown-copy probes.
Single observed C7 snapshot times were 27.57 ms (small) and 27.61 ms (grown);
these are individual observations, not a benchmark or bake-time estimate.

## Reproduction and handoff

Remote root:
`baypier-prod:/root/llmbase-rust-migration-20260920/c7-visibility-headers-20261014`.
Build/test session `llmbase-c7-build` ran [remote-run.sh](primary/remote-run.sh);
proof session `llmbase-c7-proof` ran [remote-proof.sh](primary/remote-proof.sh)
in baseline-only and candidate-only phases. Both runners completed with exit 0.
Logs are in the root; while active, use `tmux capture-pane -pt <session> -S -160`.
The proof scripts refuse to overwrite existing corpus copies.

This is an unreleased upstream fix, suitable for downstream R3 retesting.
No version bump, PyPI publication, production switch, latest-corpus full bake or
broader Goal resume is claimed. C6's explicit Siwen adapter and the remaining
dual-site/release gates still apply. Other operations' full-content limits have
not been changed by this visibility-only repair.
