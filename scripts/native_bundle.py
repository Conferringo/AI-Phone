"""Validate the actual macOS client bundle before copying it into dist."""
import hashlib
import json
from pathlib import Path
import plistlib
import re
from release_package import SECRET


def inspect_macos_bundle(root, bundle):
    config_path = root/'apps/client/src-tauri/tauri.conf.json'
    config = json.loads(config_path.read_text())
    version = json.loads((root/'config/release-policy.json').read_text())['version']
    if config['version'] != version:
        raise ValueError('Client and release versions differ')
    info = plistlib.loads((bundle/'Contents/Info.plist').read_bytes())
    for field, expected in [('CFBundleIdentifier', config['identifier']), ('CFBundleShortVersionString', version),
                            ('LSMinimumSystemVersion', config['bundle']['macOS']['minimumSystemVersion'])]:
        if info.get(field) != expected: raise ValueError('Unexpected bundle metadata: '+field)
    executable = info['CFBundleExecutable']
    if Path(executable).name != executable: raise ValueError('Invalid bundle executable')
    allowed = {'Contents/Info.plist', 'Contents/MacOS/'+executable, 'Contents/Resources/icon.icns'}
    for source, destination in config['bundle']['resources'].items():
        content = (config_path.parent/source).read_bytes()
        name = 'Contents/Resources/'+destination
        if (bundle/name).read_bytes() != content: raise ValueError('Stale/missing bundle resource: '+destination)
        allowed.add(name)
    files = []
    for path in sorted(bundle.rglob('*')):
        if path.is_symlink(): raise ValueError('Unreviewed bundle symlink')
        if not path.is_file(): continue
        name = path.relative_to(bundle).as_posix()
        if name not in allowed: raise ValueError('Unexpected bundle file: '+name)
        content = path.read_bytes()
        if SECRET.search(content) or re.search(rb'/Users/[A-Za-z0-9._-]+/', content):
            raise ValueError('Private material in bundle file: '+name)
        files.append({'path': name, 'size': len(content), 'sha256': hashlib.sha256(content).hexdigest()})
    if {file['path'] for file in files} != allowed: raise ValueError('Incomplete client bundle')
    return {'bundle_id': config['identifier'], 'version': version, 'minimum_macos': info['LSMinimumSystemVersion'], 'files': files}
