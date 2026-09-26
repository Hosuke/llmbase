"""Primary receipt audit, run from the implementation worktree."""
import hashlib
import json
from pathlib import Path
import tarfile

task = Path('.rust-migration-runs/b10-migration-old-reader')
primary = Path('.rust-migration-runs/b10-primary')
collected = primary / 'collected'
summary = json.loads((collected / 'summary.json').read_text())
digest = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
assert (collected / 'verify.exit').read_text().strip() == '0'
assert summary['passed'] is True and len(summary['steps']) == 34
assert summary['script_sha256'] == digest('scripts/rust-migration/verify-migration.py')
assert summary['script_sha256'] == digest(collected / 'verify-migration.py')
assert (collected / 'inputs-actual.sha256').read_bytes() == (collected / 'inputs-after.sha256').read_bytes()
assert summary['native_source_manifest_sha256'] == digest(task / 'accepted-source.sha256')
assert summary['native_source_manifest_sha256'] == digest(collected / 'accepted-source.sha256')
assert summary['legacy_archive_sha256'] == digest(task / 'legacy-a7c4d9d.tar.gz')
assert summary['legacy_source_manifest_sha256'] == digest(task / 'legacy-source-sha256.json')
manifest = json.loads((task / 'legacy-source-sha256.json').read_text())
with tarfile.open(task / 'legacy-a7c4d9d.tar.gz') as archive:
    actual = {m.name: hashlib.sha256(archive.extractfile(m).read()).hexdigest()
              for m in archive.getmembers() if m.isfile()}
assert actual == manifest and len(manifest) == 46
assert summary['legacy_reader_baseline'] == summary['legacy_reader_after_rollback']
modules = summary['legacy_reader_after_rollback']['modules']
assert 'llmwiki._core' not in modules
for module in modules.values():
    assert module['under_legacy'] is True
    relative = module['file'].split('/legacy-source/', 1)[1]
    assert module['sha256'] == manifest[relative]
assert summary['original_inventory_before'] == summary['original_inventory_after']
assert summary['configuration_preserved'] is True
assert summary['provider_fixture_requests'] == []
nonzero = [(s['note'], s['exit']) for s in summary['steps'] if s['exit'] != 0]
assert nonzero == [('resume copy must fail on conflict', 1), ('negative verify must fail', 1)]
assert not any(s.get('timed_out') for s in summary['steps'])
jobs = [json.loads(s['stdout']) for s in summary['steps'] if s['note'] == 'op job_get']
assert jobs[0] == jobs[1] == summary['retained']['job']
resolutions = [json.loads(s['stdout']) for s in summary['steps'] if s['note'] == 'op evidence']
assert all(r['evidence'] == summary['retained']['evidence'] for r in resolutions)
assert resolutions[-1]['deleted'] and resolutions[-1]['stale']
assert summary['get_deleted_source'] == dict(summary['retained']['revision_after_update'], deleted=True)
assert summary['resume_contract']['final_state'] == 'complete'
assert json.loads((primary / 'timeout-record.json').read_text())['passed'] is True
count = 0
for line in (task / 'accepted-source.sha256').read_text().splitlines():
    expected, path = line.split(maxsplit=1)
    assert digest(path.lstrip('*')) == expected, path
    count += 1
assert count == 119
print(json.dumps({'passed': True, 'steps': 34, 'native_sources': count,
                  'legacy_files': len(manifest), 'legacy_modules_after_calls': len(modules),
                  'preserved_original_files': sum('sha256' in v for v in summary['original_inventory_after'].values()),
                  'expected_negative_exits': nonzero}, ensure_ascii=False, indent=2))
