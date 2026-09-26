import hashlib
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile

root = Path(__file__).resolve().parents[3]
bundle = root / 'docs/rust-migration/checkpoints/native-config-2026-09-27'
manifest = json.loads((bundle / 'files-sha256.json').read_text())
with tempfile.TemporaryDirectory(prefix='llmbase-b16-replay-') as temporary:
    replay = Path(temporary)
    with tarfile.open(bundle / 'baseline-files.tar.gz') as archive:
        archive.extractall(replay, filter='data')
    for name, digests in manifest.items():
        path = replay / name
        if digests['before'] is not None:
            assert hashlib.sha256(path.read_bytes()).hexdigest() == digests['before'], name
        else:
            assert not path.exists(), name
    subprocess.run(['git', 'apply', '--check', str(bundle / 'change.patch')], cwd=replay, check=True)
    subprocess.run(['git', 'apply', str(bundle / 'change.patch')], cwd=replay, check=True)
    for name, digests in manifest.items():
        actual = (replay / name).read_bytes()
        assert hashlib.sha256(actual).hexdigest() == digests['after'], name
        assert actual == (root / name).read_bytes(), name
result = {'passed': True, 'files': len(manifest), 'outside_git': True}
(Path(__file__).parent / 'replay-result.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result))
