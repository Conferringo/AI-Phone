"""Build deterministic source-only resources; no network, history or user state."""
import gzip
import hashlib
import io
import json
from pathlib import Path
import shutil
import tarfile
from release_package import select_files

ROOT = Path(__file__).resolve().parent.parent


def prepare(root=ROOT):
    folder = root / 'apps/client/src-tauri/gen/host'; folder.mkdir(parents=True, exist_ok=True)
    files = select_files(root)
    manifest = {'format': 1, 'files': [dict(path=name, size=len(data), sha256=hashlib.sha256(data).hexdigest(), mode=mode) for name, data, mode in files]}
    files.append(('HOST-MANIFEST.json', json.dumps(manifest, sort_keys=True).encode(), 0o644))
    archive = folder / 'source.payload'
    with archive.with_suffix('.tmp').open('wb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', mtime=0, filename='') as compressed, tarfile.open(fileobj=compressed, mode='w') as target:
        for name, data, mode in files:
            entry = tarfile.TarInfo(name); entry.mode = mode; entry.size = len(data); entry.mtime = 0
            target.addfile(entry, io.BytesIO(data))
    archive.with_suffix('.tmp').replace(archive)
    shutil.copyfile(root / 'scripts/client_host_entry.py', folder / 'entry.py')
    digests = {name: hashlib.sha256((folder / name).read_bytes()).hexdigest() for name in ['source.payload', 'entry.py']}
    (folder / 'integrity.json').write_text(json.dumps(digests, sort_keys=True))
    return digests


if __name__ == '__main__': print(json.dumps(prepare()))
