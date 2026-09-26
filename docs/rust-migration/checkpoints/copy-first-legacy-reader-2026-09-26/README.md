# Copy-first migration and actual legacy-reader acceptance — 2026-09-26

Accepted bounded B10 change: a reproducible migration verifier and its runbook.
Kimi K3 implemented the specified task; the primary reviewed it, strengthened
provenance and state assertions, and independently reran all 34 child commands
on baypier-prod. Both the primary remote verifier and receipt audit exited 0.
This checkpoint does not claim all Python parity or dual-site release readiness.

## Verified behavior

- A synthetic custom-root, bare-body Classical Chinese library retains all 10
  original files byte-for-byte. Unknown metadata, accepted concept/output,
  manual taxonomy, aliases/backlinks, binary asset, pipeline and cache bytes
  survive copy, backup and restore. Non-path configuration/profile is unchanged.
- Real native dry-run/copy/verify/restore/backup/rollback operations execute.
  Working/restored copies are verified before native runtime access changes
  them; original migration and backup snapshots remain immutable.
- A real finite index job succeeds. A scratch source is created, searched,
  revised and deleted. Restore retains the entire completed job DTO, entire
  deleted source DTO and full historical Evidence (including library, revision,
  passage and original byte span). Deleted source passages remain unsearchable.
- Before and after rollback, a separate pristine baseline Python subprocess
  calls actual load_config, export_article and search_raw. Results agree. All
  six llmwiki modules loaded after those calls originate in the baseline source
  and hash-match the archive; no llmwiki._core is loaded. The primary audit also
  derives all 46 file hashes directly from the archive and matches the manifest.
- Simulated incomplete-receipt copy rejects a seeded conflict without overwriting
  it, then resumes after only that fixture conflict is corrected. A tampered
  throwaway snapshot fails verify. Both expected failures exit 1; all other
  child invocations exit 0. These are not a measured SIGKILL experiment.
- No model request occurs. Python belongs to the verifier/legacy-reader boundary;
  interpreter-free native operation is separately evidenced by B9.

## Artifacts and provenance

- `change.patch`, `baseline-files.tar.gz`, `files-sha256.json`: exact two-file
  replay; the verifier is new and therefore absent from the before archive.
- `primary/summary.json`, `verify.log`, `verify.exit`, `verify.sh`: accepted
  independent run at `/root/llmbase-rust-migration-20260920/b10-primary`.
- `primary/audit.py`, `audit.json`: hashes, archive/module provenance, all command
  exits, both original inventories, whole job/Evidence/deleted-source equality.
  The audit runs from the implementation worktree with the recorded private
  `.rust-migration-runs` layout; public reproduction uses the runbook command.
- `primary/timeout-record.json`: a deliberately timed-out short subprocess
  remains a raised failure and its attempted command/partial output are retained.
- `inputs/`: frozen fixture, pristine legacy archive, manifests and exact K3 task.
- `first-pass/`: K3's verbatim report, four attempt receipts/logs and runner;
  its initial acceptance language is superseded by this primary review. The
  first three failures were verifier path mapping, a nonunique search token,
  and a CompletedProcess attribute typo. Their results were not discarded.

Native binary SHA256:
`f1b2bdd98971fe3be42774ee3349fb9bb73af9d4ee89072ee7c804a14c8753d8`.
It is the accepted B8 release. All 119 current native source hashes match its
accepted manifest; this task changes no engine or facade source. No redundant
native full-workspace build was run.

Primary review corrected module checks occurring before lazy imports, added
actual archive/manifest hashes and all-file checks, replaced informational
deleted-get output with an exact contract assertion, compared whole job and
Evidence values, checked configuration preservation and saved the final original
inventory. Timeout capture preserves failure rather than returning success.

Validation: Python syntax, TypeScript `tsc --noEmit`, Python lint import,
independent remote proof, primary audit, exact patch replay and staged source
review pass. Primary review is authorized by the user's explicit choice of
reviewer. No unresolved HIGH findings; LGTM for this bounded checkpoint.
Staged whitespace validation excludes the nested replay patch and raw logs;
one original failed-run log deliberately retains its trailing space verbatim.

K3 exited 0; its PID and local session ended. The primary finite remote runner
exited 0 and closed its fixture server/children. No task runtime remains needed.
No production corpus or original downstream working tree was modified. Broader
facade parity, fresh candidate wheel and same-candidate Siwen/Taixu acceptance
remain required before release.
