"""Minimal native-installer entry point. Only the bundled, hashed source is executable."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import sys
import tarfile
import tempfile


def source_name(name):
    path = PurePosixPath(name)
    if not name or '\\' in name or path.is_absolute() or str(path) != name or any(p in ('..', '.', 'data', 'user', '.git', '.tools', 'target', 'node_modules') for p in path.parts) or any(p.startswith('.env') for p in path.parts) and name != '.env.example':
        raise ValueError('Invalid source payload path')
    return path


def stage_source(archive, installation):
    archive, installation = Path(archive), Path(installation)
    with archive.open('rb') as stream: digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    installation.mkdir(parents=True, exist_ok=True, mode=0o700)
    if installation.is_symlink(): raise ValueError('Installation directory cannot be a symlink')
    releases = installation / 'releases'; releases.mkdir(exist_ok=True)
    user = installation / 'user'; user.mkdir(exist_ok=True, mode=0o700)
    if releases.is_symlink() or user.is_symlink(): raise ValueError('Managed directories cannot be symlinks')
    data = user / 'data'; data.mkdir(exist_ok=True, mode=0o700)
    if data.is_symlink(): raise ValueError('User data cannot be a symlink')
    destination = releases / digest
    with tarfile.open(archive, 'r:gz') as source:
        entries = source.getmembers()
        if not 1 <= len(entries) <= 20000 or any(not entry.isfile() or entry.size > 16 * 1024 * 1024 for entry in entries) or sum(entry.size for entry in entries) > 200 * 1024 * 1024:
            raise ValueError('Invalid source payload entries')
        names = [entry.name for entry in entries]
        if len(set(names)) != len(names) or 'HOST-MANIFEST.json' not in names: raise ValueError('Source manifest missing or duplicated')
        manifest = json.load(source.extractfile('HOST-MANIFEST.json'))
        if manifest.get('format') != 1 or not isinstance(manifest.get('files'), list): raise ValueError('Invalid source manifest')
        records = {record['path']: record for record in manifest['files']}
        if len(records) != len(manifest['files']) or set(records) != set(names) - {'HOST-MANIFEST.json'}: raise ValueError('Source manifest does not match payload')
        for name in records: source_name(name)
        def validate_existing():
            if destination.is_symlink(): raise ValueError('Existing source changed; repair requires review')
            for name, record in records.items():
                path = destination / name
                if any(p.is_symlink() for p in [path, *path.parents] if p.is_relative_to(destination)) or not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != record['sha256']:
                    raise ValueError('Existing source changed; user data was preserved')
            for name, target in [('data', data), ('.env', user / '.env')]:
                link = destination / name
                if not link.is_symlink() or link.resolve() != target.resolve(): raise ValueError('Persistent user directory changed; no files were replaced')
        if destination.exists():
            validate_existing(); return destination
        with tempfile.TemporaryDirectory(prefix='.staging-', dir=releases) as temp:
            work = Path(temp) / 'source'; work.mkdir()
            for name, record in records.items():
                content = source.extractfile(name).read()
                if len(content) != record['size'] or hashlib.sha256(content).hexdigest() != record['sha256'] or record['mode'] not in (0o644, 0o755): raise ValueError('Source payload integrity check failed')
                path = work / name; path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(content); path.chmod(record['mode'])
            (work / 'HOST-MANIFEST.json').write_text(json.dumps(manifest))
            (work / 'data').symlink_to(data, target_is_directory=True)
            (work / '.env').symlink_to(user / '.env')
            work.rename(destination)
    return destination


def emit(phase, state='running', **values):
    print('AI_PHONE_SETUP ' + json.dumps(dict(phase=phase, state=state, **values)), flush=True)


def main():
    import argparse
    parser = argparse.ArgumentParser(); parser.add_argument('--payload', type=Path, required=True); parser.add_argument('--home', type=Path, required=True); parser.add_argument('--request', type=Path, required=True); parser.add_argument('--mise', type=Path, required=True)
    args = parser.parse_args()
    import fcntl
    import signal
    def interrupted(*_): raise SystemExit(130)
    signal.signal(signal.SIGTERM, interrupted)
    args.home.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd = os.open(args.home / 'bootstrap.lock', os.O_RDWR | os.O_CREAT, 0o600)
    with os.fdopen(fd, 'a+') as lock:
        try: fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError: raise ValueError('Another native installer owns this user directory')
        emit('source')
        root = stage_source(args.payload, args.home)
        sys.path.insert(0, str(root / 'scripts'))
        from client_host import install
        install(root, args.home, json.loads(args.request.read_text()), args.mise, emit)


if __name__ == '__main__':
    try: main()
    except (ValueError, OSError) as error:
        # Detailed subprocess output goes to the private native installer log.
        print(str(error), file=sys.stderr)
        emit('failed', 'failed', error=getattr(error, 'install_code', 'installation_failed'))
        sys.exit(1)
