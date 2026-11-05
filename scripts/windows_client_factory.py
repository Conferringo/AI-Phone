"""Maintainer assembly of one Windows EXE with native Android and an owned WSL backend."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import sys

from artifact_io import digest
from host_platform import host_key
from linux_client_factory import run

BACKEND = {'web', 'native-services', 'phone-apps', 'js-services', 'guest-apps'}
BODY = 'wsl-backend.payload'
METADATA = 'windows-backend.json'


def source_identity(root):
    source = os.environ.get('GITHUB_SHA', '')
    if re.fullmatch('[0-9a-f]{40}', source) is None:
        raise ValueError('Complete Windows factory requires an exact source commit')
    return source, digest(Path(root)/'SOURCE-MANIFEST.json')


def stage_backend(root, report, payload):
    from package_delivery import file_record
    root, payload = Path(root), Path(payload)
    source, inventory = source_identity(root)
    if (report.get('source_commit') != source or report.get('passed') is not True
            or report.get('component', {}).get('provenance', {}).get('source_components') is None
            or set(report['component']['provenance']['source_components']) != BACKEND):
        raise ValueError('Backend must qualify the current source with both phone app groups')
    runtime = report.get('runtime', {})
    phases = runtime.get('phases', [])
    if (runtime.get('passed') is not True or len(phases) != 2
            or any(not phase.get('checks') or not all(value is True for value in phase['checks'].values()) for phase in phases)
            or runtime.get('cleanup') != {'container': True, 'image': True, 'volume': True}):
        raise ValueError('Backend startup, restart and owned cleanup must qualify')
    from wsl_backend import verify_capsule
    if verify_capsule(payload) != report['component']:
        raise ValueError('Qualified backend capsule changed')
    folder = root/'dist/windows-backend'
    folder.mkdir()
    target = folder/BODY
    shutil.copyfile(payload, target)
    record = file_record(root, target)
    value = {'format': 1, 'source_commit': source, 'source_manifest_sha256': inventory,
             'binary_publication': False, 'user_state_bundled': False,
             'backend_components': sorted(BACKEND), 'artifact': record}
    path = folder/METADATA
    path.write_text(json.dumps(value, indent=2)+'\n')
    return path


def admit_backend(root, folder, expected):
    from package_delivery import checked_report, file_record
    root, folder = Path(root), Path(folder)
    source, _inventory = source_identity(root)
    path = folder/METADATA
    if (folder.is_symlink() or path.is_symlink() or not path.is_file()
            or path.stat().st_size > 16384 or digest(path) != expected
            or {p.name for p in folder.iterdir()} != {METADATA, BODY}):
        raise ValueError('Backend handoff inventory changed')
    value = json.loads(path.read_text())
    checked_report(root, value, source)
    if value.get('user_state_bundled') is not False or value.get('backend_components') != sorted(BACKEND):
        raise ValueError('Incomplete Windows backend handoff')
    payload = folder/BODY
    # file_record also rejects aliases, empty files and over-limit handoffs.
    record = file_record(root, payload)
    if record != value['artifact']:
        raise ValueError('Backend handoff bytes changed')
    from wsl_backend import verify_capsule
    component = verify_capsule(payload)
    if set(component.get('provenance', {}).get('source_components', [])) != BACKEND:
        raise ValueError('Backend is missing offline phone applications')
    return payload, component


def checked_delivery(components):
    from client_delivery import delivery_metadata
    value = delivery_metadata(components, 'windows-x64')
    if not value['complete_device_bundle']:
        raise ValueError('Complete Windows client needs both matched Android and WSL resources')
    return value


def prepare_android(root, work):
    from android_component_factory import build, qualify
    payload, receipt = build(root, work/'android', compression_preset=9)
    installation = work/'android-qualification'
    receipt['qualification'] = qualify(root, payload, installation, receipt)
    shutil.rmtree(installation)
    for role in ('emulator', 'platform-tools', 'system-image'):
        (work/'android'/(role+'.zip')).unlink()
    return payload, receipt


def compile_client(root, android, backend):
    from prepare_client_host import payload_components
    from client_delivery import installer_metadata
    run([sys.executable, root/'scripts/build_client.py', '--platform', 'windows',
         '--android-runtime', android, '--wsl-runtime', backend], root)
    checked_delivery(payload_components(root))
    version = json.loads((root/'config/release-policy.json').read_text())['version']
    path = root/'dist'/f'AI-Phone-{version}-windows-x64-setup.exe'
    manifest = json.loads(path.with_suffix('.exe.manifest.json').read_text())
    delivery = checked_delivery(manifest['prebuilt_components'])
    if not manifest['complete_device_bundle'] or manifest['sha256'] != digest(path):
        raise ValueError('Compiled Windows installer changed or is incomplete')
    installer = installer_metadata(path, 'windows-x64')
    if not installer['github_asset_eligible']:
        raise ValueError('Complete Windows EXE exceeds the single-file release limit')
    return {'file': path.name, 'size': path.stat().st_size, 'sha256': manifest['sha256'], 'delivery': delivery}


def build(root, folder, identity):
    root = Path(root).resolve()
    if host_key() != 'windows-x64':
        raise ValueError('Complete Windows assembly requires a native Windows x64 builder')
    from check_source import check
    check(root, repository=True, manifest=True)
    source, inventory = source_identity(root)
    backend, component = admit_backend(root, folder, identity)
    work = root/'.tools/complete-windows'
    work.mkdir()
    android, phone = prepare_android(root, work)
    print(json.dumps({'stage': 'bundle-resources', 'android_runtime_bytes': android.stat().st_size,
                      'wsl_backend_bytes': backend.stat().st_size}), flush=True)
    artifact = compile_client(root, android, backend)
    from package_delivery import prepare
    package = prepare(root, artifact)
    if source_identity(root) != (source, inventory):
        raise ValueError('Source inventory changed during complete Windows assembly')
    report = {'format': 1, 'host': 'windows-x64', 'source_commit': source,
              'source_manifest_sha256': inventory, 'artifact': artifact,
              'package_delivery': package, 'android': phone,
              'backend_rootfs': component['rootfs'],
              'physical_installation': 'not-tested', 'windows_boot_verified': False,
              'binary_publication': False}
    (root/'dist/windows-full-factory-report.json').write_text(json.dumps(report, indent=2)+'\n')
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['stage-backend', 'build'])
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--backend', type=Path, required=True)
    parser.add_argument('--report', type=Path)
    parser.add_argument('--identity')
    args = parser.parse_args()
    if args.operation == 'stage-backend':
        if not args.report:parser.error('stage-backend requires the actual qualification report')
        path = stage_backend(args.root, json.loads(args.report.read_text()), args.backend)
        print(json.dumps({'metadata': str(path), 'sha256': digest(path)}))
    else:
        if not args.identity:parser.error('build requires the producing job metadata digest')
        print(json.dumps(build(args.root, args.backend, args.identity)))
