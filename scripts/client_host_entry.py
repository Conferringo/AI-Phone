"""Native installer staging. User state stays outside verified, versioned payloads."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import sys
import tarfile
import tempfile
import re

COMPONENT_MANIFESTS={'web':'runtime/manifest.json','native-services':'runtime/services/manifest.json','phone-apps':'runtime/phone-apps/manifest.json','container-host':'runtime/container-host/manifest.json','js-services':'runtime/js-services/manifest.json','guest-apps':'runtime/guest-apps/manifest.json'}

def canonical_name(name):
    if not isinstance(name, str):
        raise ValueError('Invalid payload path')
    if set(name).intersection('\\:') or any((ord(c) < 32 for c in name)):
        raise ValueError('Invalid payload path')
    path = PurePosixPath(name)
    if not path.parts:
        raise ValueError('Invalid payload path')
    if path.is_absolute() or path.as_posix() != name or '..' in path.parts:
        raise ValueError('Invalid payload path')
    return path

def runtime_name(path, enabled):
    if not enabled:
        return False
    return len(path.parts) > 1 and path.parts[0] == 'runtime' and (path.parts[1] in ('web', 'bin', 'manifest.json', 'services', 'phone-apps', 'container-host', 'js-services', 'guest-apps'))

def source_name(name, runtime=False):
    path = canonical_name(name)
    forbidden = {'.git', '.tools'}
    if not runtime_name(path, runtime):
        forbidden.update(('data', 'user', 'target', 'node_modules', 'runtime'))
    elif 'node_modules' not in path.parts:
        forbidden.update(('data', 'user'))
    if forbidden.intersection(path.parts):
        raise ValueError('Invalid source payload path')
    if name != '.env.example' and any((p.startswith('.env') for p in path.parts)):
        raise ValueError('Invalid source payload path')
    return path

def link_rows(root, manifest_name='runtime/manifest.json'):
    manifest = Path(root) / manifest_name
    if manifest.stat().st_size > 4 * 1024 * 1024:
        raise ValueError('Invalid runtime link manifest')
    rows = json.loads(manifest.read_text()).get('links', [])
    if not isinstance(rows, list) or len(rows) > 20000:
        raise ValueError('Invalid runtime links')
    return rows

def parse_link(row, component='runtime/web'):
    if not isinstance(row, dict) or set(row) != {'path', 'target'}:
        raise ValueError('Invalid runtime link record')
    if not all((isinstance(v, str) for v in row.values())):
        raise ValueError('Invalid runtime link record')
    path = source_name(row['path'], runtime=True)
    target = row['target']
    if not row['path'].startswith(component + '/'):
        raise ValueError('Invalid runtime link')
    if set(target).intersection('\\:') or PurePosixPath(target).is_absolute():
        raise ValueError('Invalid runtime link')
    return (path, target)

def link_destination(root, name, target, component='runtime/web'):
    component = root / component
    link = root / name
    parents = [link.parent, *link.parent.parents]
    if any((p.is_symlink() for p in parents if p.is_relative_to(component))):
        raise ValueError('Runtime link parent is not owned')
    destination = (link.parent / target).resolve()
    if link.parent.resolve().is_relative_to(destination) or not destination.is_relative_to(component.resolve()):
        raise ValueError('Runtime link target escapes')
    if not destination.exists():
        raise ValueError('Runtime link target is missing')
    return (link, destination)

def ensure_link(link, destination, target, create):
    if link.is_symlink():
        if link.resolve() != destination:
            raise ValueError('Runtime link changed')
    elif link.exists():
        raise ValueError('Runtime link was replaced')
    elif create:
        link.parent.mkdir(parents=True, exist_ok=True)
        link.symlink_to(Path(target), target_is_directory=destination.is_dir())
    else:
        raise ValueError('Runtime link missing')

def web_links(root, *, create=False):
    """Aliases are contained; create them only after all regular files are verified."""
    component_links(root, 'runtime/manifest.json', 'runtime/web', create=create)

def component_links(root, manifest_name, component, *, create=False):
    root = Path(root).resolve()
    seen = set()
    for row in link_rows(root, manifest_name):
        path, target = parse_link(row, component)
        if path in seen:
            raise ValueError('Duplicate runtime link')
        seen.add(path)
        link, destination = link_destination(root, path, target, component)
        ensure_link(link, destination, target, create)

def file_hash(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def verify_file(root, record):
    path = root / record['path']
    if any((p.is_symlink() for p in [path, *path.parents] if p.is_relative_to(root))):
        raise ValueError('Payload integrity check failed')
    if not path.is_file():
        raise ValueError('Payload integrity check failed')
    if path.stat().st_size != record['size'] or file_hash(path) != record['sha256']:
        raise ValueError('Payload integrity check failed')

def persistent_layout(installation):
    installation.mkdir(parents=True, exist_ok=True, mode=448)
    if installation.is_symlink():
        raise ValueError('Installation directory cannot be a symlink')
    installation = installation.resolve()
    folders = [installation / 'releases', installation / 'user', installation / 'user/data']
    for path in folders:
        if path.is_symlink():
            raise ValueError('Managed directories cannot be symlinks')
        path.mkdir(exist_ok=True, mode=448)
    return (folders[0], folders[2])

def checked_record(row):
    if not isinstance(row, dict) or set(row) != {'path', 'size', 'sha256', 'mode'}:
        raise ValueError('Invalid source record')
    if not isinstance(row['path'], str) or type(row['size']) is not int:
        raise ValueError('Invalid source record')
    if row['size'] < 0 or row['mode'] not in (420, 493):
        raise ValueError('Invalid source record')
    if not isinstance(row['sha256'], str) or not re.fullmatch('[0-9a-f]{64}', row['sha256']):
        raise ValueError('Invalid source record')
    return row

def manifest_components(manifest):
    if not isinstance(manifest, dict):
        raise ValueError('Invalid source manifest')
    components = manifest.get('components', [])
    runtime = manifest.get('format') == 2 and isinstance(components,list) and bool(components) and components==[name for name in COMPONENT_MANIFESTS if name in components]
    if type(manifest.get('format')) is not int or not (manifest['format'] == 1 or runtime):
        raise ValueError('Invalid source manifest')
    return components, runtime

def payload_records(manifest, entries):
    components, runtime = manifest_components(manifest)
    rows = manifest.get('files')
    if not isinstance(rows, list):
        raise ValueError('Invalid source manifest')
    records = {row['path']: row for row in map(checked_record, rows)}
    names = [entry.name for entry in entries]
    if len(records) != len(rows) or set(records) != set(names) - {'HOST-MANIFEST.json'}:
        raise ValueError('Source manifest does not match payload')
    if runtime:
        component_inventory(records, components)
    return (records, runtime)

def component_inventory(records, components):
    if not {COMPONENT_MANIFESTS[name] for name in components}.issubset(records):
        raise ValueError('Runtime component manifest is missing')
    for name in records:
        if name.startswith('runtime/'):
            owner = next((role for role,path in reversed(COMPONENT_MANIFESTS.items()) if name.startswith(str(PurePosixPath(path).parent)+'/')))
            if owner not in components:
                raise ValueError('Runtime file belongs to an undeclared component')

def bounded_entries(source):
    entries = []
    total = 0
    for entry in source:
        total += entry.size
        if not entry.isfile() or not 0 <= entry.size <= 200 * 1024 * 1024:
            raise ValueError('Invalid source payload entries')
        if len(entries) >= 20000 or total > 1024 * 1024 * 1024:
            raise ValueError('Invalid source payload entries')
        entries.append(entry)
    if not entries:
        raise ValueError('Invalid source payload entries')
    return entries

def archive_records(source):
    entries = bounded_entries(source)
    names = [entry.name for entry in entries]
    if len(set(names)) != len(names) or 'HOST-MANIFEST.json' not in names:
        raise ValueError('Source manifest missing or duplicated')
    if source.getmember('HOST-MANIFEST.json').size > 4 * 1024 * 1024:
        raise ValueError('Invalid source manifest size')
    manifest = json.load(source.extractfile('HOST-MANIFEST.json'))
    records, runtime = payload_records(manifest, entries)
    validate_entry_sizes(entries, records, runtime)
    return (manifest, records, runtime)

def validate_entry_sizes(entries, records, runtime):
    if not runtime and sum((e.size for e in entries)) > 200 * 1024 * 1024:
        raise ValueError('Invalid source payload entries')
    for name, record in records.items():
        source_name(name, runtime)
        if not name.startswith('runtime/') and record['size'] > 16 * 1024 * 1024:
            raise ValueError('Invalid source entry size')

def verify_existing(destination, records, data, runtime):
    if destination.is_symlink():
        raise ValueError('Existing source changed; repair requires review')
    try:
        for record in records.values():
            verify_file(destination, record)
    except ValueError as error:
        raise ValueError('Existing source changed; user data was preserved') from error
    if runtime:
        if (destination / 'runtime/manifest.json').is_file():
            web_links(destination)
        if (destination / 'runtime/js-services/manifest.json').is_file():
            component_links(destination, 'runtime/js-services/manifest.json', 'runtime/js-services')
    for name, target in [('data', data), ('.env', data.parent / '.env')]:
        verify_persistent_link(destination / name, target)

def verify_persistent_link(link, target):
    if not link.is_symlink() or link.resolve() != target.resolve():
        raise ValueError('Persistent user directory changed; no files were replaced')

def write_payload(source, work, records):
    for name, record in records.items():
        content = source.extractfile(name).read()
        if len(content) != record['size'] or hashlib.sha256(content).hexdigest() != record['sha256']:
            raise ValueError('Source payload integrity check failed')
        path = work / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
        path.chmod(record['mode'])

def stage_source(archive, installation):
    archive = Path(archive)
    releases, data = persistent_layout(Path(installation))
    destination = releases / file_hash(archive)
    with tarfile.open(archive, 'r:gz') as source:
        manifest, records, runtime = archive_records(source)
        if destination.exists():
            verify_existing(destination, records, data, runtime)
            return destination
        with tempfile.TemporaryDirectory(prefix='.staging-', dir=releases) as temp:
            work = Path(temp) / 'source'
            work.mkdir()
            write_payload(source, work, records)
            (work / 'HOST-MANIFEST.json').write_text(json.dumps(manifest))
            if runtime:
                if (work / 'runtime/manifest.json').is_file():
                    web_links(work, create=True)
            if (work / 'runtime/js-services/manifest.json').is_file():
                component_links(work, 'runtime/js-services/manifest.json', 'runtime/js-services', create=True)
            (work / 'data').symlink_to(data, target_is_directory=True)
            (work / '.env').symlink_to(data.parent / '.env')
            work.rename(destination)
    return destination

def emit(phase, state='running', **values):
    print('AI_PHONE_SETUP ' + json.dumps(dict(phase=phase, state=state, **values)), flush=True)

def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument('--payload', type=Path, required=True)
    parser.add_argument('--home', type=Path, required=True)
    parser.add_argument('--request', type=Path, required=True)
    parser.add_argument('--mise', type=Path)
    parser.add_argument('--tool-runtime',type=Path)
    parser.add_argument('--android-runtime',type=Path)
    parser.add_argument('--images-runtime',type=Path)
    args = parser.parse_args()
    if args.mise is None and args.tool_runtime is None:parser.error('Supply bundled installer tools or a managed tool executable')
    import fcntl
    import signal

    def interrupted(*_):
        raise SystemExit(130)
    signal.signal(signal.SIGTERM, interrupted)
    args.home.mkdir(parents=True, exist_ok=True, mode=448)
    fd = os.open(args.home / 'bootstrap.lock', os.O_RDWR | os.O_CREAT, 384)
    with os.fdopen(fd, 'a+') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError('Another native installer owns this user directory')
        emit('source')
        root = stage_source(args.payload, args.home)
        sys.path.insert(0, str(root / 'scripts'))
        from client_host import install
        android_sdk=None
        if args.android_runtime:
            from android_runtime import stage
            emit('sdk-packages')
            android_sdk=stage(args.android_runtime,args.home)
        install(root, args.home, json.loads(args.request.read_text()), args.mise, emit, android_sdk=android_sdk, image_runtime=args.images_runtime, tool_runtime=args.tool_runtime)
if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError) as error:
        print(str(error), file=sys.stderr)
        emit('failed', 'failed', error=getattr(error, 'install_code', 'installation_failed'))
        sys.exit(1)
