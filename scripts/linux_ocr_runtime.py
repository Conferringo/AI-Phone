"""Build and verify a Linux x64 OCR closure without installing host packages."""
import hashlib
import json
from pathlib import Path
import shutil
import tarfile
import tempfile
from artifact_io import digest
from client_host_entry import canonical_name, checked_record, verify_file
from host_platform import host_key

PREFIX = 'runtime/services/ocr/'
MAX_FILES = 2048
MAX_BYTES = 256 * 1024**2
RESERVE = 4 * 1024**3
LOADER = 'lib/ld-linux-x86-64.so.2'
REQUIRED = {'bin/tesseract', LOADER, 'lib/libtesseract.so.5', 'tessdata/eng.traineddata',
            'tessdata/chi_sim.traineddata', 'tessdata/configs/tsv', 'python/PIL/__init__.py',
            'python/PIL/_imaging.cpython-313-x86_64-linux-gnu.so',
            'source-records.json', 'licenses/tesseract-ocr.copyright'}
LAUNCHERS = {'run.py': ('run.py', 0o644), 'launchers/tesseract': ('tesseract.sh', 0o755)}
SOURCE = ('config/linux-ocr.json', 'scripts/linux_ocr_runtime.py', 'scripts/phone-vision/tesseract.py',
          'scripts/linux-ocr/run.py', 'scripts/linux-ocr/tesseract.sh', 'scripts/linux-ocr/vision.sh',
          'scripts/linux-ocr/Dockerfile', 'scripts/linux-ocr/collect.py')


def source_fingerprint(root):
    rows = [(name, digest(Path(root) / name)) for name in SOURCE]
    return hashlib.sha256(json.dumps(rows, separators=(',', ':')).encode()).hexdigest()


def pin(root):
    value = json.loads((Path(root) / 'config/linux-ocr.json').read_text())
    if not isinstance(value, dict) or type(value.get('format')) is not int or value['format'] != 1 or value.get('host') != 'linux-x64':
        raise ValueError('Unsupported OCR host pin')
    row = value.get('archive')
    if not isinstance(row, dict): raise ValueError('Missing OCR archive pin')
    checked_record({'path': row.get('file'), 'size': row.get('size_bytes'), 'sha256': row.get('sha256'), 'mode': 0o644})
    if Path(row['file']).name != row['file'] or not 0 < row['size_bytes'] <= MAX_BYTES:
        raise ValueError('Invalid OCR archive pin')
    return row


def payload_path(name):
    path = canonical_name(name)
    if path.parts[0] not in {'bin', 'lib', 'tessdata', 'python', 'licenses', 'source-records.json'}:
        raise ValueError('Unexpected OCR archive path')
    if len(path.parts) > 16: raise ValueError('OCR path exceeds depth budget')
    return path


def elf_check(path):
    with path.open('rb') as source: header = source.read(64)
    if len(header) < 64 or header[:7] != b'\x7fELF\x02\x01\x01' or header[18:20] != b'\x3e\x00':
        raise ValueError('OCR ELF does not match Linux x64')


def archive_member(entry, seen, total):
    name = payload_path(entry.name).as_posix(); total += entry.size
    if name in seen or len(seen) >= MAX_FILES or not entry.isfile() or entry.size < 0 or total > MAX_BYTES:
        raise ValueError('Invalid OCR archive member or budget')
    return name, total


def write_member(source, entry, folder, name):
    target = folder / name; target.parent.mkdir(parents=True, exist_ok=True)
    with source.extractfile(entry) as data, target.open('wb') as output: shutil.copyfileobj(data, output)
    native = name.startswith(('bin/', 'lib/')) or name.endswith('.so')
    if native: elf_check(target)
    target.chmod(0o755 if native else 0o644)


def extract(archive, folder):
    seen = set(); total = 0
    with tarfile.open(archive, 'r|gz') as source:
        for entry in source:
            name, total = archive_member(entry, seen, total); seen.add(name)
            write_member(source, entry, folder, name)
    if not REQUIRED <= seen: raise ValueError('Incomplete OCR dependency closure')


def inventory(folder):
    rows = []
    for path in sorted(folder.rglob('*')):
        if path.is_symlink(): raise ValueError('OCR member cannot be an alias')
        if path.is_dir(): continue
        if not path.is_file(): raise ValueError('Invalid OCR file type')
        rows.append({'path': path.relative_to(folder).as_posix(), 'size': path.stat().st_size,
                     'sha256': digest(path), 'mode': path.stat().st_mode & 0o777})
        if len(rows) > MAX_FILES: raise ValueError('OCR file budget exceeded')
    return rows


def bundled_records(rows):
    if not isinstance(rows, list) or not 1 <= len(rows) <= MAX_FILES: raise ValueError('Invalid OCR inventory')
    records = [checked_record(row) for row in rows]
    names = {row['path'] for row in records}
    if len(names) != len(records) or any(not name.startswith(PREFIX) for name in names):
        raise ValueError('Invalid OCR inventory paths')
    if sum(row['size'] for row in records) > MAX_BYTES: raise ValueError('OCR inventory exceeds byte budget')
    return records


def read_manifest(root, folder, host):
    if folder.is_symlink(): raise ValueError('OCR component cannot be an alias')
    path = folder / 'manifest.json'
    if path.is_symlink() or path.stat().st_size > 1024**2: raise ValueError('Invalid OCR manifest')
    value = json.loads(path.read_text())
    checked_identity(value, host)
    if value.get('source_sha256') != source_fingerprint(root) or value.get('archive_sha256') != pin(root)['sha256']:
        raise ValueError('OCR component source changed; rebuild before packaging')
    rows = value.get('files')
    if not isinstance(rows, list) or not 1 <= len(rows) <= MAX_FILES or not all(isinstance(row, dict) for row in rows):
        raise ValueError('Invalid OCR inventory')
    return value


def checked_identity(value, host):
    if not isinstance(value, dict) or type(value.get('format')) is not int or value['format'] != 1 or value.get('host') != host or host != 'linux-x64':
        raise ValueError('OCR component does not match this host')


def verify(root, folder, *, host=None):
    folder = Path(folder); value = read_manifest(root, folder, host or host_key())
    rows = bundled_records([{**row, 'path': PREFIX + row['path']} for row in value['files']])
    actual = {row['path'] for row in inventory(folder)}
    expected = {row['path'][len(PREFIX):] for row in rows}
    if actual != expected | {'manifest.json'}: raise ValueError('OCR file inventory changed')
    if not REQUIRED | {'run.py', 'vision.py', 'launchers/tesseract'} <= expected:
        raise ValueError('Incomplete OCR runtime inventory')
    for row in value['files']:
        verify_file(folder, row)
        if (folder / row['path']).stat().st_mode & 0o777 != row['mode']: raise ValueError('OCR file permissions changed')
    return value


def build(root, archive, destination):
    root = Path(root); archive = Path(archive); destination = Path(destination); source = pin(root)
    if destination.exists() or destination.is_symlink(): return verify(root, destination, host='linux-x64')
    if archive.is_symlink() or archive.stat().st_size != source['size_bytes'] or digest(archive) != source['sha256']:
        raise ValueError('OCR archive integrity mismatch')
    if shutil.disk_usage(root).free < RESERVE + MAX_BYTES: raise ValueError('Keep 4 GiB free while building OCR')
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.ocr-', dir=destination.parent) as temporary:
        folder = Path(temporary); extract(archive, folder)
        for target, (name, mode) in LAUNCHERS.items():
            path = folder / target; path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(root / 'scripts/linux-ocr' / name, path); path.chmod(mode)
        shutil.copyfile(root / 'scripts/phone-vision/tesseract.py', folder / 'vision.py')
        (folder / 'vision.py').chmod(0o644)
        value = {'format': 1, 'host': 'linux-x64', 'source_sha256': source_fingerprint(root),
                 'archive_sha256': source['sha256'], 'files': inventory(folder)}
        (folder / 'manifest.json').write_text(json.dumps(value, sort_keys=True) + '\n')
        (folder / 'manifest.json').chmod(0o644); verify(root, folder, host='linux-x64')
        folder.rename(destination)
    return value


def prepare(root, component):
    component = Path(component); verify(root, component)
    return [(PREFIX + row['path'], (component / row['path']).read_bytes(), row['mode']) for row in inventory(component)]


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--destination', type=Path, required=True)
    args = parser.parse_args()
    value = build(args.root, args.archive, args.destination)
    print(json.dumps({'host': value['host'], 'files': len(value['files']), 'bytes': sum(row['size'] for row in value['files'])}))
