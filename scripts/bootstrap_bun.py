"""Install the pinned Bun executable directly; no npm or global installation."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parent.parent


def bun_path(root=ROOT):
    return root / '.tools/bin' / ('bun.exe' if os.name == 'nt' else 'bun')


def asset_name(system=None, machine=None):
    system, machine = system or platform.system(), machine or platform.machine().lower()
    arch = {'arm64': 'aarch64', 'aarch64': 'aarch64', 'amd64': 'x64', 'x86_64': 'x64'}.get(machine)
    host = {'Darwin': 'darwin', 'Linux': 'linux', 'Windows': 'windows'}.get(system)
    if not arch or not host:
        raise ValueError('Unsupported Bun host: ' + system + '/' + machine)
    return 'bun-' + host + '-' + arch + ('-baseline' if arch == 'x64' else '')


def install(root=ROOT, download=None):
    policy = json.loads((root / 'config/bun-runtime.json').read_text())
    destination = bun_path(root)
    if destination.is_file():
        version = subprocess.check_output([destination, '--version'], text=True).strip()
        if version == policy['version']:
            return destination
        raise ValueError('Project Bun differs from the pinned version; no executable replaced')
    name = asset_name()
    digest = policy['assets'][name + '.zip']
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.bun-', dir=destination.parent) as temp:
        archive = Path(temp) / 'bun.zip'
        url = 'https://github.com/oven-sh/bun/releases/download/bun-v' + policy['version'] + '/' + name + '.zip'
        if download:
            download(url, archive)
        else:
            with urllib.request.urlopen(url, timeout=120) as response, archive.open('wb') as output:
                shutil.copyfileobj(response, output)
        if hashlib.sha256(archive.read_bytes()).hexdigest() != digest:
            raise ValueError('Bun archive checksum mismatch; installation cancelled')
        executable = 'bun.exe' if os.name == 'nt' else 'bun'
        with zipfile.ZipFile(archive) as bundle:
            entry = bundle.getinfo(name + '/' + executable)
            if entry.file_size == 0:
                raise ValueError('Empty Bun executable')
            if entry.file_size >= 200 * 1024 * 1024:
                raise ValueError('Unexpected Bun executable size')
            staged = Path(temp) / executable
            with bundle.open(entry) as source, staged.open('wb') as output:
                shutil.copyfileobj(source, output)
        staged.chmod(0o755)
        if subprocess.check_output([staged, '--version'], text=True).strip() != policy['version']:
            raise ValueError('Downloaded Bun reports an unexpected version')
        staged.replace(destination)
    return destination


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dependencies', action='store_true')
    args = parser.parse_args()
    binary = install()
    if args.dependencies:
        subprocess.run([binary, 'install', '--frozen-lockfile'], cwd=ROOT, check=True)
    print(binary)
