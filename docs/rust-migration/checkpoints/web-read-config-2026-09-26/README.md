# Raw-preview configuration fixture repair

Accepted one-file checkpoint following b42def8. Primary diagnosed an invalid
test input, K3 implemented the specified repair, and primary reviewed the exact
diff and independently verified all 119 remote source inputs against the current
worktree. No production code or content-cap expectation changed.

With serde_json arbitrary_precision enabled, passing a serde_json::Value to
serde_yaml serialized the integer 3 as a private Number mapping. The loader
correctly read a mapping, so the configured cap did not apply. The fixture now
uses the loader's supported JSON syntax and asserts a genuine integer. A new
literal YAML test separately verifies cap 3, non-BMP truncation to `𠮷原典`, and
unchanged original raw bytes. Every existing response assertion remains.

Actual remote results, baypier-prod isolated b8-web-read-config:

- Before: web_read 3 pass / 1 fail, exit 101, original truncation assertion.
- After: web_read 5 pass, document_yaml_contracts 8 pass, each exit 0.
- Workspace fmt, all-target Clippy with warnings denied, and full workspace
  tests `--locked --no-fail-fast`: each exit 0, no failed target.
- Actual release CLI build: exit 0, 1m39s, version `llmbase 0.1.0`.
- Primary's post-run 119-file source hash verification: exit 0; only the owned
  test differs from the frozen input snapshot. Frontend TypeScript and Python
  lint import pass; source-package replay passes.

Release artifact retained remotely at
`/root/llmbase-rust-migration-20260920/b8-web-read-config/llmbase-cli-bin`:
SHA256 `f1b2bdd98971fe3be42774ee3349fb9bb73af9d4ee89072ee7c804a14c8753d8`.
It was freshly built after the complete fixed check matrix; it is not a published
package. HTTP/MCP interpreter-free proof, fresh wheel/downstream acceptance,
legacy-reader rollback and remaining facade parity gates are still separate.

The first run had a synchronization destination error: the repaired test did
not land at the overlay path, so nominal fixed steps actually used the original
test. Those records remain as `*.pass1.*` and are not accepted fixed evidence.
The second run adds a checked overlay, verifies its hash, and passes all checks.
Primary also verifies the whole final snapshot, not merely the overlay hash.

All B8 local/remote runners have ended. No push, version bump, publication or
production switch occurred. Full migration sources are not yet a complete Git
release; this bundle saves the exact before bytes and replayable one-file delta.

Replay: extract baseline-files.tar.gz into a new scratch directory, verify the
before hash, run `git apply --check change.patch` and `git apply change.patch`,
then compare the after hash in files-sha256.json with the working source.
