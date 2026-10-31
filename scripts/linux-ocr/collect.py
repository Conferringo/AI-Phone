"""Maintainer factory: preserve ELF closure, wheel and distribution attribution."""
import gzip
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import zipfile

PILLOW_SHA = '5f56499c1a74d2b862beb0c33b8016f99d095858455df9d21583bbbc1c15b582'


def sha(path):
    with path.open('rb') as source: return hashlib.file_digest(source, 'sha256').hexdigest()


def native_closure(root):
    files = {}
    names = subprocess.check_output(['/usr/bin/lddtree', '-l', '/usr/bin/tesseract'], text=True).splitlines()
    for name in names:
        source = Path(name); target = root / ('bin/tesseract' if name == '/usr/bin/tesseract' else 'lib/' + source.name)
        identity = sha(source)
        if target.exists() and sha(target) != identity: raise ValueError('Conflicting ELF library names')
        shutil.copyfile(source, target); target.chmod(0o755)
        files[name] = {'sha256': identity, 'target': target.relative_to(root).as_posix()}
    return files


def assemble(root):
    for name in ('bin', 'lib', 'tessdata', 'licenses'): (root / name).mkdir()
    files = native_closure(root)
    for language in ('eng', 'chi_sim', 'osd'):
        shutil.copyfile('/usr/share/tesseract-ocr/5/tessdata/' + language + '.traineddata', root / 'tessdata' / (language + '.traineddata'))
    (root / 'tessdata/configs').mkdir()
    shutil.copyfile('/usr/share/tesseract-ocr/5/tessdata/configs/tsv', root / 'tessdata/configs/tsv')
    wheel = next(root.glob('pillow*.whl'))
    if sha(wheel) != PILLOW_SHA: raise ValueError('Pillow wheel differs from the reviewed PyPI bytes')
    with zipfile.ZipFile(wheel) as source: source.extractall(root / 'python')
    packages = subprocess.check_output(['dpkg-query', '-W', '-f=${Package} ${Version}\n'], text=True).splitlines()
    for package in packages:
        name = package.split()[0]; source = Path('/usr/share/doc') / name / 'copyright'
        if source.is_file(): shutil.copyfile(source, root / 'licenses' / (name + '.copyright'))
    value = {'native_files': files, 'packages': packages, 'wheel': {'file': wheel.name, 'sha256': PILLOW_SHA}}
    wheel.unlink()
    (root / 'source-records.json').write_text(json.dumps(value, sort_keys=True) + '\n')


def archive(root, destination):
    with destination.open('wb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', mtime=0, filename='') as compressed, tarfile.open(fileobj=compressed, mode='w') as target:
        for path in sorted(root.rglob('*')):
            if not path.is_file(): continue
            data = path.read_bytes(); entry = tarfile.TarInfo(path.relative_to(root).as_posix())
            entry.size = len(data); entry.mode = path.stat().st_mode & 0o777
            target.addfile(entry, io.BytesIO(data))


if __name__ == '__main__':
    root = Path('/out'); assemble(root)
    destination = Path('/export'); destination.mkdir()
    archive(root, destination / 'linux-x64-ocr.tar.gz')
