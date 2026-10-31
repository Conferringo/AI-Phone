"""Authenticated Lima, clean ISO and offline engine packages for a native host."""
import argparse
import hashlib
import json
from pathlib import Path
import shlex
import shutil
import tarfile
import tempfile
from artifact_io import digest
from client_host_entry import canonical_name, checked_record, verify_file
from host_platform import host_key
from macos_target import minimum_version, verify_targets, version_parts

PREFIX = 'runtime/container-host/'
LIMIT = 512 * 1024**2
RESERVE = 4 * 1024**3


def policy(root):
    value = json.loads((Path(root) / 'config/container-host-runtime.json').read_text())
    host = host_key()
    if value.get('format') != 1 or host not in value.get('hosts', {}):
        raise ValueError('No matched container-host component for this platform')
    runtime = value['hosts'][host]
    bootstrap = json.loads((Path(root) / 'config/host-bootstrap.json').read_text())
    result = {'host': host, 'runtime': runtime, 'lima': bootstrap['lima_hosts'][host]}
    if host.startswith('macos-'): result['minimum_macos'] = minimum_version(root)
    return result


def policy_digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def pinned_file(path, record):
    path = Path(path)
    if path.is_symlink() or not path.is_file():
        raise ValueError('Container-host input is not a regular file')
    if 'size_bytes' in record and path.stat().st_size != record['size_bytes']:
        raise ValueError('Container-host input size differs')
    if digest(path) != record['sha256']:
        raise ValueError('Container-host input checksum differs')


def package_inputs(folder, rows):
    expected = {row['file'] for row in rows}
    if len(expected) != len(rows) or {p.name for p in Path(folder).iterdir()} != expected:
        raise ValueError('Incomplete container-host package closure')
    for row in rows:
        if Path(row['file']).name != row['file'] or not row['file'].endswith('.apk'):
            raise ValueError('Invalid container package filename')
        pinned_file(Path(folder) / row['file'], row)


def archive_entries(source):
    total = 0
    for count, entry in enumerate(source):
        total += entry.size
        if count >= 2048 or not 0 <= entry.size <= 200 * 1024**2 or total > LIMIT:
            raise ValueError('Container-host archive exceeds its budget')
        path = archive_path(entry)
        if path is not None:
            yield entry, path


def archive_path(entry):
    if entry.isdir() and entry.name in ('.', './'):
        return None
    name = entry.name.removeprefix('./').rstrip('/')
    path = canonical_name(name)
    if entry.issym() and name == 'share/doc/lima/templates' and entry.linkname == '../../lima/templates':
        return None
    if not (entry.isfile() or entry.isdir()):
        raise ValueError('Invalid container-host archive member')
    return path


def unpack_lima(archive, destination):
    with tarfile.open(archive, 'r:gz') as source:
        for entry, name in archive_entries(source):
            if entry.isdir():
                continue
            target = destination / name
            target.parent.mkdir(parents=True, exist_ok=True)
            with source.extractfile(entry) as body, target.open('xb') as output:
                shutil.copyfileobj(body, output, 1024**2)
            target.chmod(0o755 if entry.mode & 0o111 else 0o644)


def file_inventory(folder):
    names = set()
    for count, path in enumerate(folder.rglob('*')):
        if count >= 2048 or path.is_symlink() or len(path.relative_to(folder).parts) > 16:
            raise ValueError('Invalid container-host resource tree')
        if path.is_file():
            names.add(path.relative_to(folder).as_posix())
        elif not path.is_dir():
            raise ValueError('Invalid container-host resource member')
    return names


def checked_manifest(folder, value):
    path = folder / 'manifest.json'
    if path.is_symlink() or path.stat().st_size > 256 * 1024:
        raise ValueError('Invalid container-host manifest')
    manifest = json.loads(path.read_text())
    if not isinstance(manifest, dict) or type(manifest.get('format')) is not int or manifest['format'] != 1:
        raise ValueError('Invalid container-host manifest')
    if manifest.get('host') != value['host'] or manifest.get('policy_sha256') != policy_digest(value):
        raise ValueError('Container-host source policy changed')
    checked_targets(manifest.get('native_targets'), value)
    validate_inventory(manifest.get('files'), value)
    return manifest


def checked_targets(targets, value):
    if value['host'] == 'linux-x64':
        if targets != {'lima': 'linux-x64'}: raise ValueError('Invalid Linux native target')
    else:
        validate_targets(targets, value['minimum_macos'])


def validate_inventory(records, value):
    if not isinstance(records, list) or not 1 <= len(records) <= 1024:
        raise ValueError('Incomplete container-host manifest')
    validate_records(records, value)


def validate_targets(targets, maximum):
    import re
    if not isinstance(targets, dict) or set(targets) != {'lima'}:
        raise ValueError('Container-host native target is missing')
    target = targets['lima']
    if not isinstance(target, str) or re.fullmatch(r'\d+\.\d+(?:\.\d+)?', target) is None:
        raise ValueError('Container-host native target is invalid')
    if version_parts(target) > version_parts(maximum):
        raise ValueError('Container-host native target exceeds the client requirement')


def validate_records(rows, value):
    records = {}
    total = 0
    for row in map(checked_record, rows):
        name = canonical_name(row['path'])
        allowed = ('lima', 'packages', 'image.iso', 'native', 'bin') if value['host'] == 'linux-x64' else ('lima', 'packages', 'image.iso')
        if name.parts[0] not in allowed or len(name.parts) > 16:
            raise ValueError('Invalid container-host path')
        total += row['size']
        if row['path'] in records or row['size'] > 200 * 1024**2 or total > LIMIT:
            raise ValueError('Invalid container-host inventory')
        records[row['path']] = row
    validate_required(records, value)


def validate_required(records, value):
    required = {'lima/bin/limactl', 'lima/share/lima/lima-guestagent.Linux-' + value['runtime']['arch'] + '.gz', 'image.iso'}
    required.update('packages/' + row['file'] for row in value['runtime']['packages'])
    if value['host'] == 'linux-x64': required.update({'bin/limactl', 'native/manifest.json'})
    if not required <= records.keys() or records['lima/bin/limactl']['mode'] != 0o755:
        raise ValueError('Container-host runtime is incomplete')
    pins = [('image.iso', value['runtime']['image'])] + [('packages/' + row['file'], row) for row in value['runtime']['packages']]
    for name, pin in pins:
        row = records[name]
        if row['size'] != pin['size_bytes'] or row['sha256'] != pin['sha256'] or row['mode'] != 0o644:
            raise ValueError('Container-host pinned resource differs')


def verify(root, folder):
    folder = Path(folder)
    if folder.is_symlink():
        raise ValueError('Container-host component cannot be a symlink')
    manifest = checked_manifest(folder, policy(root))
    if file_inventory(folder) != {'manifest.json', *[row['path'] for row in manifest['files']]}:
        raise ValueError('Container-host inventory changed')
    for row in manifest['files']:
        verify_file(folder, row)
        if (folder / row['path']).stat().st_mode & 0o777 != row['mode']:
            raise ValueError('Container-host resource permissions changed')
    if manifest['host'] == 'linux-x64':
        from linux_host_runtime import verify as verify_native
        from linux_dependencies import elf_check
        verify_native(root, folder / 'native'); elf_check(folder / 'lima/bin/limactl')
    return manifest


def build(root, lima, image, packages):
    root = Path(root).resolve()
    value = policy(root)
    pinned_file(lima, value['lima'])
    pinned_file(image, value['runtime']['image'])
    package_inputs(packages, value['runtime']['packages'])
    output = root / '.tools/container-host'
    if output.exists():
        verify(root, output)
        return {'reused': True, 'policy_sha256': policy_digest(value)}
    output.parent.mkdir(parents=True, exist_ok=True)
    if shutil.disk_usage(output.parent).free < RESERVE + LIMIT:
        raise ValueError('Keep 4 GiB free while building the container host')
    with tempfile.TemporaryDirectory(prefix='.container-host-', dir=output.parent) as temporary:
        work = Path(temporary) / 'component'
        work.mkdir()
        unpack_lima(lima, work / 'lima')
        replace_native(root, work, value)
        targets = prepare_native(root, work, value)
        shutil.copyfile(image, work / 'image.iso')
        shutil.copytree(packages, work / 'packages')
        records = [{'path': name, 'size': (work / name).stat().st_size,
                    'sha256': digest(work / name), 'mode': (work / name).stat().st_mode & 0o777}
                   for name in sorted(file_inventory(work))]
        manifest = {'format': 1, 'host': value['host'], 'policy_sha256': policy_digest(value), 'native_targets': targets, 'files': records}
        (work / 'manifest.json').write_text(json.dumps(manifest, sort_keys=True) + '\n')
        verify(root, work)
        work.rename(output)
    return {'files': len(records), 'bytes': sum(row['size'] for row in records), 'policy_sha256': policy_digest(value)}


def prepare_native(root, work, value):
    if value['host'] != 'linux-x64': return verify_targets(root, {'lima': work / 'lima/bin/limactl'})
    from linux_host_runtime import prepare, PREFIX as native_prefix
    from linux_dependencies import elf_check
    elf_check(work / 'lima/bin/limactl')
    for name, data, mode in prepare(root, root / '.tools/linux-host-runtime'):
        path = work / 'native' / name.removeprefix(native_prefix)
        path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(data); path.chmod(mode)
    (work / 'bin').mkdir(); command = work / 'bin/limactl'
    shutil.copyfile(root / 'scripts/linux-host/lima.sh', command); command.chmod(0o755)
    return {'lima': 'linux-x64'}


def replace_native(root, work, value):
    if 'lima_build' not in value['runtime']:
        return
    from lima_native import cached_build
    receipt = cached_build(root)
    if receipt is None:
        raise ValueError('Build the matched native Lima runtime before packaging')
    shutil.copyfile(Path(root) / '.tools/lima-native/limactl', work / 'lima/bin/limactl')


def prepare_host(root):
    root = Path(root)
    folder = root / '.tools/container-host'
    manifest = verify(root, folder)
    return [(PREFIX + name, (folder / name).read_bytes(), 0o644 if name == 'manifest.json' else row['mode'])
            for name, row in [('manifest.json', {})] + [(row['path'], row) for row in manifest['files']]]


def selection(root):
    root = Path(root)
    folder = root / 'runtime/container-host'
    if not folder.exists() and not folder.is_symlink():
        return None
    verify(root, folder)
    value = policy(root)['runtime']
    linux = policy(root)['host'] == 'linux-x64'
    return {'command': folder / ('bin/limactl' if linux else 'lima/bin/limactl'), 'image': folder / 'image.iso',
            'vm_type': 'qemu' if linux else 'vz', 'mount_type': '9p' if linux else 'virtiofs',
            'packages': folder / 'packages', 'arch': value['arch'], 'image_sha256': value['image']['sha256']}


def definition(home, runtime, memory):
    packages = shlex.quote(str(runtime['packages']))
    script = '#!/bin/sh\nset -eu\npackages=' + packages + '\napk --no-network add "$packages"/*.apk\nrc-service docker start\n'
    return {'minimumLimaVersion': '2.2.1', 'vmType': runtime.get('vm_type', 'vz'), 'arch': runtime['arch'],
            'cpus': 4, 'memory': memory, 'disk': '64GiB',
            'images': [{'location': str(runtime['image']), 'arch': runtime['arch'], 'digest': 'sha256:' + runtime['image_sha256']}],
            'mountType': runtime.get('mount_type', 'virtiofs'), 'mounts': [{'location': str(home), 'writable': True}],
            'containerd': {'system': False, 'user': False}, 'propagateProxyEnv': False,
            'ssh': {'loadDotSSHPubKeys': False, 'forwardAgent': False},
            'env': {key: '' for key in ('HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'http_proxy', 'https_proxy', 'all_proxy')},
            'provision': [{'mode': 'dependency', 'skipDefaultDependencyResolution': True, 'script': '#!/bin/sh\nexit 0\n'},
                          {'mode': 'system', 'script': script}],
            'probes': [{'script': '#!/bin/sh\nsudo docker info >/dev/null 2>&1\n', 'hint': 'The local container engine must be ready'}],
            'hostResolver': {'hosts': {'host.docker.internal': 'host.lima.internal'}},
            'portForwards': [{'guestPortRange': [17860, 17869], 'ignore': True}]}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--lima', type=Path, required=True)
    parser.add_argument('--image', type=Path, required=True)
    parser.add_argument('--packages', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(build(args.root, args.lima, args.image, args.packages)))
