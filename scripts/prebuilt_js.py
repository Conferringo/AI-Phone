"""Source-bound JS services and their relocatable locked dependency component."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from artifact_io import digest
from client_host_entry import checked_record, component_links, source_name, verify_file
from host_platform import host_key
from release_package import select_files

PREFIX = 'runtime/js-services/'
MANIFEST = PREFIX + 'manifest.json'
MAX_FILES = 12000
MAX_BYTES = 256 * 1024**2
MAX_SOURCE = 64 * 1024**2
RESERVE = 4 * 1024**3
ENTRIES = {'media': 'apps/device-stream/server.ts', 'channels': 'apps/channels/server.ts',
           'native-apps': 'apps/device-network/native-apps.ts', 'messaging-browser': 'apps/channels/install-browser.ts',
           'device-setup': 'scripts/client-device-setup.ts', 'backend-qualification': 'scripts/wsl-desktop-qualification.ts'}
BROWSER_SOURCES=tuple('scripts/'+name+'.py' for name in ('channel_browser','browser_engine','browser_lease','browser_pipe','installer_lock','private_files','windows_private'))


def entries():
    return {role: PREFIX + str(Path(name).with_suffix('.js')).replace('\\', '/') for role, name in ENTRIES.items()}


def source_inputs(root):
    root = Path(root)
    exact = {'package.json', 'bun.lock', 'config/bun-runtime.json', 'config/js-runtime-vendor.json', 'scripts/js_dependencies.py', 'scripts/js_vendor.py', 'scripts/prebuilt_js.py', 'scripts/macos_target.py', 'apps/client/src-tauri/tauri.conf.json',
             *BROWSER_SOURCES,*ENTRIES.values()}
    prefixes = ('apps/channels/', 'apps/device-stream/', 'apps/device-network/', 'packages/protocol/')
    for name, data, mode in select_files(root):
        if name in exact or name.startswith(prefixes) and not name.endswith('.test.ts'):
            yield name, data, mode


def source_fingerprint(root):
    selected = [[name, hashlib.sha256(data).hexdigest()] for name, data, _ in source_inputs(root)]
    return hashlib.sha256(json.dumps(sorted(selected), separators=(',', ':')).encode()).hexdigest()


def snapshot_sources(root, destination):
    total = 0
    for name, data, mode in source_inputs(root):
        total += len(data)
        if total > MAX_SOURCE:
            raise ValueError('JS source snapshot exceeds its budget')
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        target.chmod(mode)


def bun_version(root):
    return json.loads((Path(root) / 'config/bun-runtime.json').read_text())['version']


def file_records(rows):
    if not isinstance(rows, list) or not 1 <= len(rows) <= MAX_FILES:
        raise ValueError('Incomplete JS runtime inventory')
    records = {}
    total = 0
    for row in map(checked_record, rows):
        source_name(row['path'], runtime=True)
        total += row['size']
        if not row['path'].startswith(PREFIX) or row['path'] in records or total > MAX_BYTES:
            raise ValueError('Invalid JS runtime inventory')
        records[row['path']] = row
    if not set(entries().values()) <= records.keys():
        raise ValueError('JS service entry is missing')
    return records


def resource_mode(path, system=None):
    if (system or os.name) == 'nt':
        return 0o755 if path.suffix.lower() in ('.exe', '.cmd', '.bat') else 0o644
    return 0o755 if path.stat().st_mode & 0o111 else 0o644


def permissions_match(path, expected, system=None):
    actual = path.stat().st_mode
    if (system or os.name) == 'nt':
        # chmod models the read-only attribute on Windows; execution uses NTFS ACLs.
        return bool(actual & 0o200) == bool(expected & 0o200)
    return actual & 0o777 == expected


def native_paths(records):
    if not host_key().startswith('macos-'):
        return set()
    return {name for name in records if Path(name).suffix in ('.node', '.dylib')}


def validate_native(root, records, targets):
    if not isinstance(targets, dict) or set(targets) != native_paths(records):
        raise ValueError('JS native dependency targets are incomplete')
    from macos_target import minimum_version, version_parts
    for target in targets.values():
        if not isinstance(target, str) or re.fullmatch(r'\d+\.\d+(?:\.\d+)?', target) is None:
            raise ValueError('Invalid JS native dependency target')
        if version_parts(target) > version_parts(minimum_version(root)):
            raise ValueError('JS dependency requires a newer macOS than the client')


def read_manifest(root, artifact):
    path = Path(artifact) / MANIFEST
    if path.is_symlink() or path.stat().st_size > 4 * 1024**2:
        raise ValueError('Invalid JS runtime manifest')
    value = json.loads(path.read_text())
    validate_header(root, value)
    return value


def validate_header(root, value):
    if not isinstance(value, dict) or type(value.get('format')) is not int or value['format'] != 1:
        raise ValueError('Invalid JS runtime manifest')
    if value.get('host') != host_key() or value.get('bun_version') != bun_version(root):
        raise ValueError('JS runtime does not match this host')
    if value.get('source_sha256') != source_fingerprint(root) or value.get('entries') != entries():
        raise ValueError('JS runtime source changed; rebuild before packaging')


def actual_members(folder):
    actual = set()
    for count, path in enumerate(folder.rglob('*')):
        if count >= MAX_FILES * 3:
            raise ValueError('JS runtime tree exceeds its budget')
        if path.is_symlink() or path.is_file():
            actual.add(path.relative_to(folder.parent.parent).as_posix())
        elif not path.is_dir():
            raise ValueError('Invalid JS runtime member')
    return actual


def verify(root, artifact):
    artifact = Path(artifact).resolve()
    folder = artifact / PREFIX
    if folder.is_symlink():
        raise ValueError('JS runtime cannot be a symlink')
    value = read_manifest(root, artifact)
    records = file_records(value.get('files'))
    validate_native(root, records, value.get('native_targets'))
    from client_host_entry import link_rows, parse_link
    links = [parse_link(row, PREFIX.rstrip('/'))[0].as_posix() for row in link_rows(artifact, MANIFEST)]
    if actual_members(folder) != set(records) | set(links) | {MANIFEST}:
        raise ValueError('JS runtime contains missing or unlisted resources')
    for row in records.values():
        verify_file(artifact, row)
        if not permissions_match(artifact / row['path'], row['mode']):
            raise ValueError('JS runtime resource permissions changed')
    component_links(artifact, MANIFEST, PREFIX.rstrip('/'))
    return value


def available(root):
    folder = Path(root) / PREFIX
    if not folder.exists() and not folder.is_symlink():
        return False
    verify(root, root)
    return True


def launch_spec(root, role):
    if role not in ENTRIES:
        raise ValueError('Unknown JS service')
    root = Path(root).resolve()
    if available(root):
        from prebuilt_web import web_command
        return {'argv': [web_command(root)[0], '--no-install', str(root / entries()[role])], 'environment': {'AI_PHONE_PROJECT_ROOT': str(root)}, 'prebuilt': True}
    return {'argv': [str(root / '.tools/bin/bun'), str(root / ENTRIES[role])], 'environment': {'AI_PHONE_PROJECT_ROOT': str(root)}, 'prebuilt': False}


def private_bun(root, *, create=False):
    root = Path(root).resolve()
    if not available(root):
        raise ValueError('Prebuilt JS component is missing')
    from prebuilt_web import web_command
    binary = Path(web_command(root)[0])
    folder = root / '.tools/bin'
    if any(path.is_symlink() for path in (folder, folder.parent)):
        raise ValueError('Private runtime tool directory was replaced')
    destination = folder / binary.name
    if destination.exists() or destination.is_symlink():
        return matching_bun(root, destination, binary)
    if not create:
        return None
    folder.mkdir(parents=True, exist_ok=True)
    if shutil.disk_usage(root).free < RESERVE + binary.stat().st_size:
        raise ValueError('Keep 4 GiB free while preparing the private Bun runtime')
    fd, name = tempfile.mkstemp(prefix='.bun-', dir=folder)
    temporary = Path(name)
    try:
        with os.fdopen(fd, 'wb') as output:
            with binary.open('rb') as source:
                shutil.copyfileobj(source, output)
        temporary.chmod(0o755)
        try:
            os.link(temporary, destination)
        except FileExistsError:
            return matching_bun(root, destination, binary)
    finally:
        temporary.unlink(missing_ok=True)
    return destination


def matching_bun(root, destination, binary):
    if not destination.resolve().is_relative_to(root) or not destination.is_file() or digest(destination) != digest(binary):
        raise ValueError('Existing private Bun was preserved; review runtime repair')
    if not os.access(destination, os.X_OK):
        raise ValueError('Private Bun is not executable')
    return destination


def prepare(root):
    root = Path(root).resolve()
    artifact = root / '.tools/js-services'
    value = verify(root, artifact)
    files = [(row['path'], (artifact / row['path']).read_bytes(), row['mode']) for row in value['files']]
    files.append((MANIFEST, (artifact / MANIFEST).read_bytes(), 0o644))
    return files


def bundle(root, work, bun):
    for role, entry in ENTRIES.items():
        target = work / entries()[role]
        target.parent.mkdir(parents=True, exist_ok=True)
        command = [str(bun), '--no-install', 'build', entry, '--target=bun', '--packages=external', '--outfile=' + str(target)]
        subprocess.run(command, cwd=root, check=True, stdout=subprocess.DEVNULL)
        target.chmod(0o644)


def copy_dependencies(root, work, graph):
    from js_dependencies import package_files
    from js_vendor import dependency_bytes, read_approvals
    approvals = read_approvals(graph.source)
    total = 0
    count = 0
    for package in graph.packages:
        physical = graph.package_source(package)
        for source in package_files(physical):
            relative = package.relative_to(root) / source.relative_to(physical)
            name = PREFIX + relative.as_posix()
            source_name(name, runtime=True)
            count += 1
            total += source.stat().st_size
            if count > MAX_FILES - len(ENTRIES) or total > MAX_BYTES:
                raise ValueError('JS dependencies exceed their resource budget')
            origin = graph.source if physical != package else root
            data = dependency_bytes(origin, physical, graph.packages[package], source, name, approvals)
            target = work / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            target.chmod(resource_mode(source))


def build_manifest(root, work, graph, source_identity):
    from prebuilt_web import public_runtime_file
    records = []
    for path in sorted((work / PREFIX).rglob('*')):
        if not path.is_file():
            continue
        name = path.relative_to(work).as_posix()
        if name in entries().values():
            public_runtime_file(root, path, Path(name), name)
        records.append({'path': name, 'size': path.stat().st_size, 'sha256': digest(path), 'mode': resource_mode(path)})
    links = [{'path': PREFIX + name, 'target': Path(os.path.relpath(work / PREFIX / target, (work / PREFIX / name).parent)).as_posix()}
             for name, target in sorted(graph.aliases.items())]
    value = {'format': 1, 'host': host_key(), 'bun_version': bun_version(root), 'source_sha256': source_identity,
             'entries': entries(), 'files': records, 'links': links}
    from macos_target import verify_targets
    value['native_targets'] = verify_targets(root, {name: work / name for name in native_paths({row['path'] for row in records})}) if host_key().startswith('macos-') else {}
    (work / MANIFEST).write_text(json.dumps(value, sort_keys=True) + '\n')
    component_links(work, MANIFEST, PREFIX.rstrip('/'), create=True)
    return value


def build(root):
    root = Path(root).resolve()
    output = root / '.tools/js-services'
    if output.exists() or output.is_symlink():
        return verify(root, output)
    from bootstrap_bun import bun_path
    from js_dependencies import collect, package_data
    bun = bun_path(root)
    compiler_version = subprocess.check_output([str(bun), '--version'], text=True).strip()
    output.parent.mkdir(parents=True, exist_ok=True)
    if shutil.disk_usage(root).free < RESERVE + MAX_BYTES + MAX_SOURCE:
        raise ValueError('Keep 4 GiB free while building JS services')
    with tempfile.TemporaryDirectory(prefix='.js-services-', dir=output.parent) as temporary:
        work = Path(temporary) / 'artifact'
        work.mkdir()
        source = Path(temporary) / 'source'
        snapshot_sources(root, source)
        source_identity = source_fingerprint(source)
        if compiler_version != bun_version(source):
            raise ValueError('Use the pinned Bun to build JS services')
        graph = collect(root, ['apps/channels', 'apps/device-stream', 'apps/device-network'], source=source)
        version = package_data(source).get('devDependencies', {}).get('@bufbuild/protobuf')
        if not isinstance(version, str):
            raise ValueError('Installer protobuf runtime declaration is missing')
        graph.add(graph.resolve(root, '@bufbuild/protobuf', version, False))
        copy_dependencies(root, work, graph)
        bundle(source, work, bun)
        if source_fingerprint(root) != source_identity:
            raise ValueError('JS sources changed during compilation; output was not published')
        value = build_manifest(source, work, graph, source_identity)
        verify(source, work)
        if source_fingerprint(root) != source_identity:
            raise ValueError('JS sources changed before publication; output was preserved only as temporary work')
        work.rename(output)
    return value


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--build', action='store_true')
    parser.add_argument('--role', choices=ENTRIES)
    parser.add_argument('--run', action='store_true')
    args, rest = parser.parse_known_args()
    if args.build:
        result = build(args.root)
        print(json.dumps({'files': len(result['files']), 'bytes': sum(row['size'] for row in result['files'])}))
    elif args.role:
        spec = launch_spec(args.root, args.role)
        if args.run:
            from runtime_config import runtime_env
            command = [*spec['argv'], *rest]
            os.execvpe(command[0], command, {**os.environ, **runtime_env(args.root), 'AI_PHONE_PROJECT_ROOT': str(args.root.resolve())})
        else:
            print(json.dumps(spec))
    else:
        parser.error('Choose --build or --role')
