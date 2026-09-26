# Accepted canonical default configuration roots

Predecessor: `a36d463`. Primary implemented and accepted this repair on2026-09-27.
When config.yaml was absent, a relative base produced roots such as
`library/wiki/concepts`. The native host joined the base prefix a second time,
so a valid atomic write failed with `PermissionError: library scope denied`.
`config._defaults` now resolves the base before constructing effective roots.
The native scope guard, writer and recovery code are unchanged.

This three-file recovery bundle contains the config delta, new regression tests
and path contract. The single before-file is in `baseline-files.tar.gz`; apply
`change.patch` and compare `files-sha256.json`. Exact replay outside Git passed.
The full untracked migration remains in the worktree and is not included here.

## Acceptance

- New tests on the accepted B14 installed candidate reproduce5 failures/5 passes.
  Relative, dot, symlink and missing-component/parent bases fail the absolute-root
  contract; direct defaults also fail. `baseline.log` preserves those failures.
- Primary review moved the optional extension import into the skipped test body,
  so an unbuilt development checkout now collects cleanly (10 skipped). Assertions
  are unchanged from the retained red baseline test. After this test-only repair,
  the complete isolated runner was repeated successfully; first-run receipts are
  retained under evidence/primary/attempt1. No runtime source changed in this repair.
- The fresh B15 wheel passes40 focused and628 full Python tests. Missing/present
  YAML, six base forms, cwd changes, read-only/non-registering loads, explicit
  registration, custom external roots, unknown/partial config and live deep-copied
  language defaults are covered.
- Real native atomic writes retain exact non-BMP/combining/CRLF bytes and the
  applied SQLite journal's exact before/after images. Out-of-root and reserved
  database writes remain rejected before mutation. No default directories are
  created by config reads, nor duplicated under a second base prefix.
- The original saved Python config/atomic implementations and repaired candidate
  both pass the relative/absolute write matrix: four exact-byte successes. The
  earlier failing candidate result is preserved separately as diagnosis evidence.
- All225 source inputs and48 installed Python files match.222 preceding inputs
  are byte-identical to B14; only config.py changes among those inputs. The
  native reuse verifier checks B14's actual manifests, successful exits and
  installed extension before retaining its fmt/Clippy/native test results.
- New wheel SHA256:
  `b79f436119c8047ff0994705836ef989113ae9f009124764cc5c7b0bc66e2b76`.
  Extension SHA256 is unchanged from B14:
  `7e557279d61616b589589630690acb0dcc28d7d12de5c310a475af6d6bbee373`.
- TypeScript noEmit and Python lint import pass. Primary reviews the exact staged
  bundle under the user's explicit authorization for direct primary review.

Remote task: `baypier-prod:/root/llmbase-rust-migration-20260920/b15-config-roots`.
Finite tmux `llmbase-b15-config-roots`, command `bash primary/launch.sh`, wraps
the recorded verifier in a1500-second timeout. Final runner exit0; its tmux
session is absent. The first-run actual pane PID573683 is also absent.
No task process remains. No model requests,
production writes, original downstream edits, dependency changes or old test edits.

`evidence/` includes exact runner/verifier scripts, manifests, logs and receipts.
The verifier needs the full matching source snapshot and isolated toolchain plus
the accepted B14 evidence at its recorded path; the recovery bundle alone is not
a standalone full source distribution. The supplied legacy modules are actual
saved originals used by the diagnostic, not reconstructed expected results.

## Remaining scope

Configuration parsing and ensure_dirs still execute Python IO. Their native
migration, remaining maintenance/catalog/worker/connector parity, latest-candidate
Siwen/Taixu integration and complete source/release gates remain required.
This checkpoint does not change native compat_io::resolve_path or establish its
general equivalence to pathlib for every missing-component/parent combination.
