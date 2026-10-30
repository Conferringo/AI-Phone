"""Validate the actual macOS client bundle before copying it into dist."""
import json
from pathlib import Path
import plistlib


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
    from client_resources import verify_compiled_inventory
    integrity=config_path.parent/'gen/host/integrity.json'
    if integrity.exists():verify_compiled_inventory(bundle/'Contents/MacOS'/executable,json.loads(integrity.read_text()))
    allowed = {'Contents/Info.plist', 'Contents/MacOS/'+executable, 'Contents/Resources/icon.icns'}
    from client_resources import resource_map, file_receipt
    for source, destination in resource_map(root).items():
        content = file_receipt(config_path.parent/source)
        name = 'Contents/Resources/'+destination
        if file_receipt(bundle/name) != content: raise ValueError('Stale/missing bundle resource: '+destination)
        allowed.add(name)
    files = []
    for path in sorted(bundle.rglob('*')):
        if path.is_symlink(): raise ValueError('Unreviewed bundle symlink')
        if not path.is_file(): continue
        name = path.relative_to(bundle).as_posix()
        if name not in allowed: raise ValueError('Unexpected bundle file: '+name)
        files.append({'path':name,**file_receipt(path)})
    if {file['path'] for file in files} != allowed: raise ValueError('Incomplete client bundle')
    return {'bundle_id': config['identifier'], 'version': version, 'minimum_macos': info['LSMinimumSystemVersion'], 'files': files}
