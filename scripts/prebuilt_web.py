"""Prepare a relocatable web runtime; credentials and user files never belong here."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
from host_platform import host_key
from release_package import SECRET
from client_host_entry import source_name, web_links, verify_file
HOME_PATH = re.compile(b'(?:/Users/[A-Za-z0-9._-]+/|[A-Za-z]:[\\\\/]+Users[\\\\/]+[^\\\\/]+[\\\\/])')

def private_path(path):
    parts = path.parts
    return any((p.startswith('.env') or p in ('.git', '.tools') for p in parts)) or ('node_modules' not in parts and bool({'data', 'user'}.intersection(parts)))

def runtime_path(name):
    path = source_name(name, runtime=True)
    if path.parts[0] != 'runtime':
        raise ValueError('Invalid web runtime path')
    return path

def relocate_config(config, root):
    for parent, key in [(config, 'repoRoot'), (config, 'outputFileTracingRoot'), (config.get('turbopack', {}), 'root'), (config.get('experimental', {}), 'outputFileTracingRoot')]:
        if key in parent:
            value = parent[key]
            if value != '.' and Path(value).resolve() != root:
                raise ValueError('Unexpected standalone build root')
            parent[key] = '.'
    return config

def next_config(text):
    marker = 'const nextConfig = '
    if marker not in text:
        raise ValueError('Standalone server configuration is missing')
    offset = text.index(marker) + len(marker)
    config, end = json.JSONDecoder().raw_decode(text[offset:])
    return (offset, config, end)

def relocate_server(data, root):
    text = data.decode('utf8')
    offset, config, end = next_config(text)
    relocate_config(config, root)
    return (text[:offset] + json.dumps(config, ensure_ascii=False) + text[offset + end:]).encode()

def relocate_build_file(data, root, name):
    if name == 'runtime/web/apps/web/server.js':
        return relocate_server(data, root)
    if name == 'runtime/web/apps/web/.next/required-server-files.json':
        value = json.loads(data)
        relocate_config(value['config'], root)
        if Path(value['appDir']).resolve() != root / 'apps/web':
            raise ValueError('Unexpected standalone application root')
        value['appDir'] = '.'
        return json.dumps(value).encode()
    return data

def unchanged_vendor(root, relative, data):
    if 'node_modules' not in relative.parts:
        return False
    upstream = root / relative
    if not upstream.is_file():
        return False
    return upstream.read_bytes() == data and str(Path.home()).encode() not in data

def public_runtime_file(root, source, relative, name):
    data = relocate_build_file(source.read_bytes(), root, name)
    if SECRET.search(data):
        raise ValueError('Private material in web runtime: ' + name)
    private_home = HOME_PATH.search(data) or (str(Path.home()).rstrip('/\\') + os.sep).encode() in data
    if private_home and (not unchanged_vendor(root, relative, data)):
        raise ValueError('Private material in web runtime: ' + name)
    return data

def runtime_sources(folder, current):
    for path in sorted(current.iterdir()):
        relative = path.relative_to(folder)
        if retired_image_dependency(relative):
            continue
        resolved = path.resolve()
        if not resolved.is_relative_to(folder):
            raise ValueError('Runtime symlink escapes its build tree')
        if private_path(relative):
            raise ValueError('Runtime includes a private path: ' + relative.as_posix())
        if path.is_symlink() or getattr(path, 'is_junction', lambda: False)():
            yield (path, relative, resolved)
        elif path.is_dir():
            yield from runtime_sources(folder, path)
        elif path.is_file():
            yield (path, relative, None)
        else:
            raise ValueError('Invalid web runtime entry')

def retired_image_dependency(path):
    name = '/' + path.as_posix() + '/'
    return any((value in name for value in ('/node_modules/sharp/', '/node_modules/@img/sharp-', '/.bun/sharp@', '/.bun/@img+sharp-')))

def collect_runtime(root, folder, prefix):
    files = []
    links = []
    folder = folder.resolve()
    if not folder.is_dir():
        return (files, links)
    for path, relative, target in runtime_sources(folder, folder):
        name = (PurePosixPath(prefix) / relative.as_posix()).as_posix()
        runtime_path(name)
        if target is not None:
            links.append({'path': name, 'target': Path(os.path.relpath(target, path.parent)).as_posix()})
        elif path.suffix != '.map' and (not path.name.endswith('.nft.json')):
            data = public_runtime_file(root, path, relative, name)
            files.append((name, data, 493 if os.access(path, os.X_OK) else 420))
    return (files, links)

def runtime_binary(root, bun):
    policy = root / 'config/bun-runtime.json'
    version = json.loads(policy.read_text())['version']
    if subprocess.check_output([bun, '--version'], text=True).strip() != version:
        raise ValueError('Web runtime requires the pinned Bun version')
    if bun.stat().st_size > 200 * 1024 * 1024:
        raise ValueError('Unexpected Bun executable size')
    binary = 'runtime/bin/bun.exe' if os.name == 'nt' else 'runtime/bin/bun'
    return (binary, bun.read_bytes(), 493)

def prepare_web(root, bun):
    root = Path(root).resolve()
    app = root / 'apps/web'
    standalone = app / '.next/standalone'
    if not (standalone / 'apps/web/server.js').is_file():
        raise ValueError('Build the standalone web application first')
    _, config, _ = next_config((standalone / 'apps/web/server.js').read_text())
    if config.get('images', {}).get('unoptimized') is not True:
        raise ValueError('Standalone runtime requires native image optimization to be disabled')
    files = []
    links = []
    for folder, prefix in [(standalone, 'runtime/web'), (app / '.next/static', 'runtime/web/apps/web/.next/static'), (app / 'public', 'runtime/web/apps/web/public')]:
        collected, aliases = collect_runtime(root, folder, prefix)
        files.extend(collected)
        links.extend(aliases)
    if any(('node_modules/sharp/' in name or 'node_modules/@img/sharp-' in name for name, _, _ in files)):
        raise ValueError('Unused native image optimizer remains in the runtime')
    files.append(runtime_binary(root, Path(bun).resolve()))
    if len({name for name, _, _ in files}) != len(files):
        raise ValueError('Duplicate web runtime file')
    records = [{'path': name, 'size': len(data), 'sha256': hashlib.sha256(data).hexdigest(), 'mode': mode} for name, data, mode in files]
    manifest = {'format': 1, 'host': host_key(), 'services': ['web'], 'links': links, 'files': records}
    return files + [('runtime/manifest.json', (json.dumps(manifest, sort_keys=True) + '\n').encode(), 420)]

def load_manifest(root):
    path = root / 'runtime/manifest.json'
    if path.is_symlink() or path.stat().st_size > 4 * 1024 * 1024:
        raise ValueError('Invalid web runtime manifest')
    manifest = json.loads(path.read_text())
    if manifest.get('format') != 1 or manifest.get('host') != host_key():
        raise ValueError('Web runtime does not match this host')
    if manifest.get('services') != ['web']:
        raise ValueError('Invalid web runtime service')
    records = manifest.get('files', [])
    if not isinstance(records, list) or not 1 <= len(records) <= 20000:
        raise ValueError('Invalid web runtime inventory')
    return records

def web_command(root):
    root = Path(root).resolve()
    names = set()
    for row in load_manifest(root):
        name = row['path']
        runtime_path(name)
        if name in names:
            raise ValueError('Duplicate web runtime inventory')
        names.add(name)
        verify_file(root, row)
    executable = 'runtime/bin/bun.exe' if os.name == 'nt' else 'runtime/bin/bun'
    entry = 'runtime/web/apps/web/server.js'
    if not {executable, entry} <= names:
        raise ValueError('Incomplete web runtime')
    web_links(root)
    return [str(root / executable), str(root / entry)]
if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args()
    print(json.dumps(web_command(args.root)))
