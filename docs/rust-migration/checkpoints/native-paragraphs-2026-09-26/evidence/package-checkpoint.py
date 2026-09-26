"""Save explicit source files and their delta for an untracked migration task."""
import difflib
import gzip
import hashlib
import io
import json
from pathlib import Path
import sys
import tarfile

before, output = map(Path, sys.argv[1:3])
names = sys.argv[3:]
output.mkdir(parents=True, exist_ok=True)
manifest, patch = {}, []


def patch_lines(data):
    # Git patches delimit lines only with LF. str.splitlines also splits literal
    # Unicode separators inside JSON strings and would create a corrupt patch.
    parts = data.decode().split('\n')
    return [line + '\n' for line in parts[:-1]] + ([parts[-1]] if parts[-1] else [])


buffer = io.BytesIO()
with tarfile.open(fileobj=buffer, mode='w') as archive:
    for name in names:
        path = Path(name)
        assert not path.is_absolute() and '..' not in path.parts
        old_path = before / path
        old = old_path.read_bytes() if old_path.is_file() else None
        new = path.read_bytes()
        manifest[name] = {
            'before': hashlib.sha256(old).hexdigest() if old is not None else None,
            'after': hashlib.sha256(new).hexdigest(),
        }
        if old is not None:
            info = tarfile.TarInfo(name)
            info.size, info.mode = len(old), 0o644
            archive.addfile(info, io.BytesIO(old))
        if old != new:
            patch.append(f'diff --git a/{name} b/{name}\n')
            if old is None:
                patch.append('new file mode 100644\n')
            for line in difflib.unified_diff(
                patch_lines(old or b''), patch_lines(new),
                fromfile='a/'+name if old is not None else '/dev/null',
                tofile='b/'+name,
            ):
                patch.append(line)
                if not line.endswith('\n'):
                    patch.append('\n\\ No newline at end of file\n')
(output/'baseline-files.tar.gz').write_bytes(gzip.compress(buffer.getvalue(), mtime=0))
(output/'change.patch').write_text(''.join(patch))
(output/'files-sha256.json').write_text(json.dumps(manifest, indent=2)+'\n')
print(f'Saved {len(names)} explicit source files and replay delta')
