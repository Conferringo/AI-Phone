"""Shared maintainer-only collection of installed ELF dependencies and attribution."""
import gzip
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import tarfile


def sha(path):
    with path.open('rb') as source: return hashlib.file_digest(source, 'sha256').hexdigest()


def copy_native(root, source, relative):
    target = root / relative; target.parent.mkdir(parents=True, exist_ok=True); identity = sha(source)
    if target.exists() and sha(target) != identity: raise ValueError('Conflicting ELF library names')
    shutil.copyfile(source, target); target.chmod(0o755)
    return {'target': relative, 'sha256': identity}


def native_closure(root, programs, modules=()):
    modules = set(modules); files = {}
    for name in [*programs.values(), *map(str, modules)]:
        names = subprocess.check_output(['/usr/bin/lddtree', '-l', name], text=True).splitlines()
        for name in names:
            source = Path(name)
            group = 'bin' if name in programs.values() else 'modules' if source in modules else 'lib'
            files[name] = copy_native(root, source, group + '/' + source.name)
    return files


def package_records(root, files, **extra):
    packages = subprocess.check_output(['dpkg-query', '-W', '-f=${Package} ${Version}\n'], text=True).splitlines()
    (root / 'licenses').mkdir()
    for package in packages:
        name = package.split()[0]; source = Path('/usr/share/doc') / name / 'copyright'
        if source.is_file(): shutil.copyfile(source, root / 'licenses' / (name + '.copyright'))
    (root / 'source-records.json').write_text(json.dumps({'native_files': files, 'packages': packages, **extra}, sort_keys=True) + '\n')


def archive(root, destination):
    with destination.open('wb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', mtime=0, filename='') as compressed, tarfile.open(fileobj=compressed, mode='w') as target:
        for path in sorted(root.rglob('*')):
            if not path.is_file(): continue
            data = path.read_bytes(); entry = tarfile.TarInfo(path.relative_to(root).as_posix())
            entry.size = len(data); entry.mode = path.stat().st_mode & 0o777
            target.addfile(entry, io.BytesIO(data))
