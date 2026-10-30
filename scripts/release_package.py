"""Allowlisted source bundles; private state never enters a release archive."""
import hashlib
import io
import json
from pathlib import Path
import re
import tarfile
import os

DIRECTORIES = ('apps', 'crates', 'packages', 'scripts', 'infra', 'config', 'skills', 'docs/public', '.github')
FILES = ('ai-phone', 'Start AI Phone.command', 'README.md', 'README.en.md', 'Cargo.toml', 'Cargo.lock', 'package.json', '.puppeteerrc.cjs', 'bun.lock', 'buf.yaml', 'buf.gen.yaml', 'compose.yaml', 'compose.dev.yaml', 'compose.desktop.yaml', '.env.example', '.gitignore', '.gitattributes', 'CONTRIBUTING.md', 'SECURITY.md', 'DISCLAIMER.md', 'PRIVACY.md', 'THIRD_PARTY_NOTICES.md', 'CHANGELOG.md', 'LICENSE', 'NOTICE')
OMIT_PARTS = {'.git', '.next', 'node_modules', 'target', 'data', '.tools', '__pycache__', 'evidence', 'dist', '.AppleDouble', '__MACOSX'}
OMIT_FILES = {'.DS_Store', 'Thumbs.db', 'desktop.ini', 'cleanup-retired-projects.py', 'persist-services.py', 'clear-requested-chat.ts', 'complete-ad-dismiss-skill.ts', 'provision-second-phone.ts', 'android-build-bridge.sh', 'android-install-bridge.sh', 'android-network-check.py', 'android-network-check.sh'}
OMIT_SUFFIXES = {'.pyc', '.log', '.dump', '.apk', '.jks', '.keystore', '.tsbuildinfo', '.qcow2', '.img', '.tar', '.gz', '.tgz', '.zip', '.xz', '.zst', '.sqlite', '.db', '.key'}
SECRET = re.compile(rb'(?<![A-Za-z0-9_-])(?:sk-[A-Za-z0-9_-]{24,}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----|gh[pousr]_[A-Za-z0-9]{30,})')
OMIT_TREES = ('apps/android-bridge', 'apps/web/src/app/lab', 'apps/client/src-tauri/gen')


def source_candidates(root):
    yield from (root/p for p in FILES if (root/p).exists())
    for name in DIRECTORIES:
        folder=root/name
        if folder.is_symlink():raise ValueError('Symlink not allowed in release: '+name)
        if not folder.exists():continue
        for base,dirs,files in os.walk(folder):
            kept=[]
            for directory in dirs:
                path=Path(base)/directory;relative=path.relative_to(root).as_posix()
                if directory in OMIT_PARTS or any(relative==p or relative.startswith(p+'/') for p in OMIT_TREES):continue
                if path.is_symlink():raise ValueError('Symlink not allowed in release: '+relative)
                kept.append(directory)
            dirs[:]=kept
            yield from (Path(base)/file for file in files)


def select_files(root):
    found = []
    for path in sorted(set(source_candidates(root))):
        rel = path.relative_to(root)
        name = rel.as_posix()
        if rel.parts[0]=='scripts' and path.name.startswith('verify-') and path.suffix in ('.ts','.py'): continue
        if any(part in OMIT_PARTS for part in rel.parts) or path.name in OMIT_FILES or path.suffix in OMIT_SUFFIXES: continue
        if path.name.startswith('._'): continue
        if path.is_symlink(): raise ValueError('Symlink not allowed in release: ' + name)
        if not path.is_file(): continue
        if path.name.startswith('.env') and name != '.env.example': continue
        if path.stat().st_size > 16 * 1024 * 1024: raise ValueError('Oversized source file: ' + name)
        data = path.read_bytes()
        if SECRET.search(data): raise ValueError('Possible credential in source file: ' + name)
        found.append((name, data, 0o755 if path.stat().st_mode & 0o111 else 0o644))
    return found


def create_package(root, output, public=False):
    root, output = Path(root).resolve(), Path(output).resolve()
    policy = json.loads((root / 'config/release-policy.json').read_text())
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]{0,79}',policy['version']): raise ValueError('Invalid release version')
    gates = policy['publication_gates']
    blocked = [name for name, passed in gates.items() if passed is not True]
    if public and blocked: raise ValueError('Publication blocked: ' + ', '.join(blocked))
    files = select_files(root)
    manifest = dict(version=policy['version'], license=policy.get('source_preview',{}).get('license'), remote_publication_authorized=False, kind='public-source-preview' if public else 'internal-source-preview', publication_ready=not blocked, blocked_gates=blocked,
                    files=[dict(path=p, sha256=hashlib.sha256(data).hexdigest(), size=len(data)) for p, data, _ in files])
    files.append(('RELEASE-MANIFEST.json', (json.dumps(manifest, indent=2) + '\n').encode(), 0o644))
    if blocked:
        files.append(('NOT-FOR-PUBLICATION.txt', b'Internal review bundle. Publication gates remain open; see RELEASE-MANIFEST.json. No user data or device disks are included.\n', 0o644))
    output.mkdir(parents=True, exist_ok=True)
    prefix = 'ai-phone-' + policy['version']
    destination = output / (prefix + '-source.tar.gz')
    temp = destination.with_suffix('.tmp')
    try:
        with tarfile.open(temp, 'w:gz') as archive:
            for name, data, mode in files:
                info = tarfile.TarInfo(prefix + '/' + name)
                info.size, info.mode, info.mtime = len(data), mode, 0
                archive.addfile(info, io.BytesIO(data))
        temp.replace(destination)
    finally:
        temp.unlink(missing_ok=True)
    digest = hashlib.sha256(destination.read_bytes()).hexdigest()
    destination.with_name(destination.name + '.sha256').write_text(digest + '  ' + destination.name + '\n')
    destination.with_name(destination.name + '.manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    return destination
