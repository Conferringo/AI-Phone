"""Verify and stage releases beside a persistent user directory. Never hot-update."""
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import tarfile
import tempfile


def stage_bundle(archive, installation):
    installation = Path(installation).resolve()
    marker = installation / 'installation.json'
    if installation.exists() and any(installation.iterdir()) and not marker.is_file():
        raise ValueError('Destination must be empty or an AI Phone managed installation')
    installation.mkdir(parents=True, exist_ok=True)
    if marker.is_file() and json.loads(marker.read_text()).get('layout_version') != 1:
        raise ValueError('Unsupported installation layout')
    for name in ('app', 'user', 'current'):
        if (installation / name).is_symlink() and name != 'current':
            raise ValueError('Managed directories may not be symlinks')
    with tarfile.open(archive, 'r:gz') as tar:
        members = tar.getmembers()
        names = set()
        if len(members) > 10000 or sum(m.size for m in members) > 256*1024*1024:
            raise ValueError('Release exceeds extraction limits')
        for m in members:
            p = PurePosixPath(m.name)
            if not m.isfile() or p.is_absolute() or '..' in p.parts or len(p.parts)<2 or m.name != p.as_posix() or m.name in names or m.size > 16*1024*1024:
                raise ValueError('Unsafe release entry')
            names.add(m.name)
        prefixes = {PurePosixPath(m.name).parts[0] for m in members}
        if len(prefixes) != 1: raise ValueError('Release must have one root')
        prefix = prefixes.pop()
        manifest_name = prefix + '/RELEASE-MANIFEST.json'
        if manifest_name not in names: raise ValueError('Release manifest missing')
        manifest = json.load(tar.extractfile(manifest_name))
        version = manifest['version']
        if not version or not all(c.isalnum() or c in '.-_' for c in version) or prefix != 'ai-phone-' + version:
            raise ValueError('Invalid release version')
        expected = {prefix + '/' + item['path']: item for item in manifest['files']}
        if len(expected) != len(manifest['files']): raise ValueError('Duplicate manifest path')
        if names - set(expected) - {manifest_name, prefix + '/NOT-FOR-PUBLICATION.txt'}:
            raise ValueError('Unlisted release files')
        if not set(expected) <= names: raise ValueError('Missing release files')
        for name, item in expected.items():
            data = tar.extractfile(name).read()
            if len(data) != item['size'] or hashlib.sha256(data).hexdigest() != item['sha256']:
                raise ValueError('Release checksum mismatch')
        releases = installation / 'app/releases'
        releases.mkdir(parents=True, exist_ok=True)
        destination = releases / version
        if destination.exists(): raise ValueError('Release version already staged')
        temp = Path(tempfile.mkdtemp(prefix='.staging-', dir=releases))
        try:
            for m in members:
                rel = PurePosixPath(m.name).relative_to(prefix)
                # No shell eval, symlinks, special files, uid or archived permissions.
                if rel.parts[0] in ('data', 'user', '.env', '.git'):
                    raise ValueError('Release contains user state')
                target = temp / str(rel)
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(tar.extractfile(m).read())
                target.chmod(0o755 if m.mode & 0o111 else 0o644)
            user = installation / 'user'
            user.mkdir(mode=0o700, exist_ok=True)
            (user / 'data').mkdir(mode=0o700, exist_ok=True)
            (temp / 'data').symlink_to(user / 'data', target_is_directory=True)
            (temp / '.env').symlink_to(user / '.env')
            temp.rename(destination)
        finally:
            if temp.exists(): shutil.rmtree(temp)
    if not marker.exists():
        marker.write_text(json.dumps({'layout_version':1, 'initial_version':version}, indent=2)+'\n')
        (installation / 'current').symlink_to(destination, target_is_directory=True)
    return destination
