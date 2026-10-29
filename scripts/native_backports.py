"""Verify exact vendored backports before classifying a known dependency advisory."""
import hashlib
import json
from pathlib import Path
import tomllib


def verified_backports(root):
    root = Path(root)
    manifest = root/'config/native-backports.json'
    if not manifest.exists():
        return {}
    config = json.loads(manifest.read_text())
    cargo = tomllib.loads((root/'apps/client/src-tauri/Cargo.toml').read_text())
    verified = {}
    for item in config['backports']:
        directory = (root/item['directory']).resolve()
        if not directory.is_relative_to((root/'apps/client/src-tauri/vendor').resolve()):
            raise ValueError('Backport must be inside the native vendor directory')
        patch = cargo.get('patch', {}).get('crates-io', {}).get(item['package'], {})
        if (root/'apps/client/src-tauri'/patch.get('path', '')).resolve() != directory:
            raise ValueError('Verified backport is not selected by Cargo')
        files = {p.relative_to(directory).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                 for p in directory.rglob('*') if p.is_file() and not p.is_symlink()}
        if any(p.is_symlink() for p in directory.rglob('*')) or files != item['files']:
            raise ValueError('Backport source differs from reviewed manifest')
        package = tomllib.loads((directory/'Cargo.toml').read_text())['package']
        if (package['name'], package['version']) != (item['package'], item['version']):
            raise ValueError('Backport package identity mismatch')
        verified[(item['package']+'@'+item['version'], item['advisory'])] = {
            'type': 'verified-upstream-backport', 'source': item['upstream_patch'],
            'manifest': 'config/native-backports.json', 'files_checked': len(files)}
    return verified
