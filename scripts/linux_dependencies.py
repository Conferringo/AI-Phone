"""Pinned Linux ELF dependency archives, inventories and atomic component assembly."""
import hashlib
import json
from pathlib import Path
import shutil
import tarfile
import tempfile
from artifact_io import digest
from client_host_entry import canonical_name, checked_record, verify_file
from host_platform import host_key

from dataclasses import dataclass

@dataclass(frozen=True)
class DependencyProfile:
    config: str
    source: tuple[str, ...]
    roots: frozenset[str]
    required: frozenset[str]
    install_files: tuple[tuple[str, str, int], ...] = ()

MAX_FILES = 2048
MAX_BYTES = 256 * 1024**2
RESERVE = 4 * 1024**3


def source_fingerprint(root, profile):
    rows = [(name, digest(Path(root) / name)) for name in profile.source]
    return hashlib.sha256(json.dumps(rows, separators=(',', ':')).encode()).hexdigest()


def pin(root, profile):
    value = json.loads((Path(root) / profile.config).read_text())
    if not isinstance(value, dict) or type(value.get('format')) is not int or value['format'] != 1 or value.get('host') != 'linux-x64':
        raise ValueError('Unsupported Linux dependency host pin')
    row = value.get('archive')
    if not isinstance(row, dict): raise ValueError('Missing Linux dependency archive pin')
    checked_record({'path': row.get('file'), 'size': row.get('size_bytes'), 'sha256': row.get('sha256'), 'mode': 0o644})
    if Path(row['file']).name != row['file'] or not 0 < row['size_bytes'] <= MAX_BYTES:
        raise ValueError('Invalid Linux dependency archive pin')
    return row


def payload_path(name, profile):
    path = canonical_name(name)
    if path.parts[0] not in profile.roots:
        raise ValueError('Unexpected Linux dependency archive path')
    if len(path.parts) > 16: raise ValueError('Linux dependency path exceeds depth budget')
    return path


def elf_check(path):
    with path.open('rb') as source: header = source.read(64)
    if len(header) < 64 or header[:7] != b'\x7fELF\x02\x01\x01' or header[18:20] != b'\x3e\x00':
        raise ValueError('Linux dependency ELF does not match Linux x64')


def archive_member(entry, seen, total, *, profile):
    name = payload_path(entry.name, profile).as_posix(); total += entry.size
    if name in seen or len(seen) >= MAX_FILES or not entry.isfile() or entry.size < 0 or total > MAX_BYTES:
        raise ValueError('Invalid Linux dependency archive member or budget')
    return name, total


def write_member(source, entry, folder, name):
    target = folder / name; target.parent.mkdir(parents=True, exist_ok=True)
    with source.extractfile(entry) as data, target.open('wb') as output: shutil.copyfileobj(data, output)
    native = name.startswith(('bin/', 'lib/')) or name.endswith('.so')
    if native: elf_check(target)
    target.chmod(0o755 if native else 0o644)


def extract(archive, folder, profile):
    seen = set(); total = 0
    with tarfile.open(archive, 'r|gz') as source:
        for entry in source:
            name, total = archive_member(entry, seen, total, profile=profile); seen.add(name)
            write_member(source, entry, folder, name)
    if not profile.required <= seen: raise ValueError('Incomplete Linux dependency closure')


def inventory(folder):
    rows = []
    for count, path in enumerate(folder.rglob('*')):
        if count >= MAX_FILES * 2 or len(path.relative_to(folder).parts) > 16:
            raise ValueError('Linux dependency tree exceeds its budget')
        if path.is_symlink(): raise ValueError('Linux dependency member cannot be an alias')
        if path.is_dir(): continue
        if not path.is_file(): raise ValueError('Invalid Linux dependency file type')
        rows.append({'path': path.relative_to(folder).as_posix(), 'size': path.stat().st_size,
                     'sha256': digest(path), 'mode': path.stat().st_mode & 0o777})
        if len(rows) > MAX_FILES: raise ValueError('Linux dependency file budget exceeded')
    return sorted(rows, key=lambda row: row['path'])


def bundled_records(rows, *, prefix):
    if not isinstance(rows, list) or not 1 <= len(rows) <= MAX_FILES: raise ValueError('Invalid Linux dependency inventory')
    records = [checked_record(row) for row in rows]
    names = {row['path'] for row in records}
    if len(names) != len(records) or any(not name.startswith(prefix) for name in names):
        raise ValueError('Invalid Linux dependency inventory paths')
    if sum(row['size'] for row in records) > MAX_BYTES: raise ValueError('Linux dependency inventory exceeds byte budget')
    return records


def read_manifest(root, folder, host, profile):
    if folder.is_symlink(): raise ValueError('Linux dependency component cannot be an alias')
    path = folder / 'manifest.json'
    if path.is_symlink() or path.stat().st_size > 1024**2: raise ValueError('Invalid Linux dependency manifest')
    value = json.loads(path.read_text())
    checked_identity(value, host)
    if value.get('source_sha256') != source_fingerprint(root, profile) or value.get('archive_sha256') != pin(root, profile)['sha256']:
        raise ValueError('Linux dependency component source changed; rebuild before packaging')
    rows = value.get('files')
    if not isinstance(rows, list) or not 1 <= len(rows) <= MAX_FILES or not all(isinstance(row, dict) for row in rows):
        raise ValueError('Invalid Linux dependency inventory')
    return value


def checked_identity(value, host):
    if not isinstance(value, dict) or type(value.get('format')) is not int or value['format'] != 1 or value.get('host') != host or host != 'linux-x64':
        raise ValueError('Linux dependency component does not match this host')


def verify(root, folder, profile, *, host=None):
    folder = Path(folder); value = read_manifest(root, folder, host or host_key(), profile)
    rows = bundled_records(value['files'], prefix='')
    actual = {row['path'] for row in inventory(folder)}
    expected = {row['path'] for row in rows}
    if actual != expected | {'manifest.json'}: raise ValueError('Linux dependency file inventory changed')
    if not profile.required | {row[0] for row in profile.install_files} <= expected:
        raise ValueError('Incomplete Linux dependency runtime inventory')
    for row in value['files']:
        verify_file(folder, row)
        if (folder / row['path']).stat().st_mode & 0o777 != row['mode']: raise ValueError('Linux dependency file permissions changed')
    return value


def build(root, archive, destination, profile):
    root = Path(root); archive = Path(archive); destination = Path(destination); source = pin(root, profile)
    if destination.exists() or destination.is_symlink(): return verify(root, destination, profile, host='linux-x64')
    if archive.is_symlink() or archive.stat().st_size != source['size_bytes'] or digest(archive) != source['sha256']:
        raise ValueError('Linux dependency archive integrity mismatch')
    if shutil.disk_usage(root).free < RESERVE + MAX_BYTES: raise ValueError('Keep 4 GiB free while building Linux dependency')
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.ocr-', dir=destination.parent) as temporary:
        folder = Path(temporary); extract(archive, folder, profile)
        for target, name, mode in profile.install_files:
            path = folder / target; path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(root / name, path); path.chmod(mode)
        value = {'format': 1, 'host': 'linux-x64', 'source_sha256': source_fingerprint(root, profile),
                 'archive_sha256': source['sha256'], 'files': inventory(folder)}
        (folder / 'manifest.json').write_text(json.dumps(value, sort_keys=True) + '\n')
        (folder / 'manifest.json').chmod(0o644); verify(root, folder, profile, host='linux-x64')
        folder.rename(destination)
    return value


def prepare(root, component, profile, *, prefix):
    component = Path(component); verify(root, component, profile)
    return [(prefix + row['path'], (component / row['path']).read_bytes(), row['mode']) for row in inventory(component)]
