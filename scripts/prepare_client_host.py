"""Build deterministic client resources; no network, history or user state."""
import gzip
import hashlib
import io
import json
from pathlib import Path
import shutil
import tarfile
from release_package import select_files
ROOT = Path(__file__).resolve().parent.parent

def prepare(root=ROOT, *, with_web_runtime=False):
    folder = root / 'apps/client/src-tauri/gen/host'
    folder.mkdir(parents=True, exist_ok=True)
    files = select_files(root)
    if with_web_runtime:
        from bootstrap_bun import bun_path
        from prebuilt_web import prepare_web
        files.extend(prepare_web(root, bun_path(root)))
    manifest = {'format': 2 if with_web_runtime else 1, 'components': ['web'] if with_web_runtime else [], 'files': [dict(path=name, size=len(data), sha256=hashlib.sha256(data).hexdigest(), mode=mode) for name, data, mode in files]}
    files.append(('HOST-MANIFEST.json', json.dumps(manifest, sort_keys=True).encode(), 420))
    archive = folder / 'source.payload'
    with archive.with_suffix('.tmp').open('wb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', mtime=0, filename='') as compressed, tarfile.open(fileobj=compressed, mode='w') as target:
        for name, data, mode in files:
            entry = tarfile.TarInfo(name)
            entry.mode = mode
            entry.size = len(data)
            entry.mtime = 0
            target.addfile(entry, io.BytesIO(data))
    archive.with_suffix('.tmp').replace(archive)
    shutil.copyfile(root / 'scripts/client_host_entry.py', folder / 'entry.py')
    digests = {name: hashlib.sha256((folder / name).read_bytes()).hexdigest() for name in ['source.payload', 'entry.py']}
    (folder / 'integrity.json').write_text(json.dumps(digests, sort_keys=True))
    return digests

def verify_prepared_web(root=ROOT):
    import tempfile
    from client_host_entry import stage_source
    from check_prebuilt_web import verify
    with tempfile.TemporaryDirectory(prefix='aiphone-bundled-web-') as folder:
        staged = stage_source(root / 'apps/client/src-tauri/gen/host/source.payload', Path(folder) / 'installation')
        return verify(staged)

def payload_components(root=ROOT):
    archive = root / 'apps/client/src-tauri/gen/host/source.payload'
    if not archive.exists():
        return []
    with tarfile.open(archive, 'r:gz') as source:
        entry = source.getmember('HOST-MANIFEST.json')
        if entry.size > 4 * 1024 * 1024:
            raise ValueError('Invalid source manifest size')
        return json.load(source.extractfile(entry)).get('components', [])
if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--with-web-runtime', action='store_true')
    parser.add_argument('--verify-web-runtime', action='store_true')
    args = parser.parse_args()
    if args.verify_web_runtime and (not args.with_web_runtime):
        parser.error('Verification requires --with-web-runtime')
    result = prepare(with_web_runtime=args.with_web_runtime)
    if args.verify_web_runtime:
        result['web_runtime_verification'] = verify_prepared_web()
    print(json.dumps(result))
