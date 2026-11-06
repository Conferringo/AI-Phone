"""Native service component inventory and verified launch selection."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
from client_host_entry import checked_record, verify_file
from host_platform import host_key
from release_package import SECRET

DEVICE_SETUP_CONTRACT = 'ai-phone-device-setup-v2'
SERVICE_SUPERVISION_CONTRACT = 'ai-phone-linux-supervision-v2'
PHONE_INPUT_SETUP_CONTRACT = 'ai-phone-input-setup-v2'
DATABASE_PREPARE_CONTRACT = 'ai-phone-database-prepare-v1'
HOST_PROFILE_CONTRACT = 'ai-phone-host-profile-v1'
MANAGED_ENGINE_CONTRACT = 'ai-phone-managed-engine-v1'
ROLES = ('assistant', 'runner', 'vision')
FIXTURE = 'runtime/services/fixtures/ocr.png'
SOURCE_FIXTURE = 'crates/phone-runner/fixtures/ocr-probe.png'
SOURCE_SUPERVISOR = 'target/debug/host-resources'


def binary_path(role):
    if role not in ROLES:
        raise ValueError('Unknown native service')
    return 'runtime/services/bin/' + role


def prepare_services(root, *, linux_ocr=None):
    from build_native_services import cached_build, source_fingerprint
    root = Path(root).resolve()
    receipt = cached_build(root, source_fingerprint(root))
    if receipt is None:
        raise ValueError('Build the matching native services before packaging')
    if receipt['checks'].get('device_setup') != DEVICE_SETUP_CONTRACT:
        raise ValueError('Native device setup capability was not qualified')
    if receipt['checks'].get('service_supervision') != SERVICE_SUPERVISION_CONTRACT:
        raise ValueError('Native service supervision capability was not qualified')
    if receipt['checks'].get('phone_input_setup') != PHONE_INPUT_SETUP_CONTRACT:
        raise ValueError('Native phone input setup capability was not qualified')
    if receipt['checks'].get('database_prepare') != DATABASE_PREPARE_CONTRACT:
        raise ValueError('Native database preparation capability was not qualified')
    if receipt['checks'].get('host_profile') != HOST_PROFILE_CONTRACT:
        raise ValueError('Native host profile capability was not qualified')
    if receipt['checks'].get('managed_engine') != MANAGED_ENGINE_CONTRACT:
        raise ValueError('Native managed engine capability was not qualified')
    files = []
    for role in ROLES:
        path = root / receipt['artifacts'][role]['path']
        data = path.read_bytes()
        if SECRET.search(data) or (str(Path.home()).rstrip('/\\') + '/').encode() in data:
            raise ValueError('Private material in native service: ' + role)
        files.append((binary_path(role), data, 0o755))
    files.append((FIXTURE, (root / SOURCE_FIXTURE).read_bytes(), 0o644))
    if linux_ocr is not None:
        from linux_ocr_runtime import prepare
        files.extend(prepare(root, linux_ocr))
    records = [{'path': name, 'size': len(data), 'sha256': hashlib.sha256(data).hexdigest(), 'mode': mode}
               for name, data, mode in files]
    manifest = {'format': 1, 'host': host_key(), 'source_sha256': receipt['source_sha256'],
                'compiler': receipt['compiler'], 'versions': {role: receipt['checks'][role]['version'] for role in ('assistant', 'runner')},
                'vision': vision_backend(linux_ocr), 'files': records, 'device_setup': DEVICE_SETUP_CONTRACT,
                'service_supervision': SERVICE_SUPERVISION_CONTRACT, 'phone_input_setup': PHONE_INPUT_SETUP_CONTRACT,
                'database_prepare': DATABASE_PREPARE_CONTRACT, 'host_profile': HOST_PROFILE_CONTRACT,
                'managed_engine': MANAGED_ENGINE_CONTRACT}
    return files + [('runtime/services/manifest.json', json.dumps(manifest, sort_keys=True).encode(), 0o644)]


def vision_backend(linux_ocr):
    if linux_ocr is not None: return 'linux-tesseract-bundled'
    return 'container-tesseract'


def read_manifest(root):
    if platform.system() not in ('Darwin', 'Linux'):
        raise ValueError('Native service host is not implemented on this platform')
    path = root / 'runtime/services/manifest.json'
    if path.is_symlink() or path.stat().st_size > 4 * 1024 * 1024:
        raise ValueError('Invalid native service manifest')
    value = json.loads(path.read_text())
    if not isinstance(value, dict) or type(value.get('format')) is not int or value.get('format') != 1 or value.get('host') != host_key():
        raise ValueError('Native services do not match this host')
    return value


def manifest_records(value):
    rows = value.get('files')
    if not isinstance(rows, list) or not len(ROLES) + 1 <= len(rows) <= 2053:
        raise ValueError('Incomplete native service inventory')
    records = [checked_record(row) for row in rows]
    extras = core_inventory(records)
    checked_extras(value, extras)
    return records


def core_inventory(records):
    core = {binary_path(role) for role in ROLES} | {FIXTURE}
    names = {row['path'] for row in records}
    if len(names) != len(records):
        raise ValueError('Duplicate native service inventory')
    if not core <= names:
        raise ValueError('Invalid native service inventory')
    extras = [row for row in records if row['path'] not in core]
    for row in (row for row in records if row['path'] in core):
        mode = 0o644 if row['path'] == FIXTURE else 0o755
        if row['mode'] != mode:
            raise ValueError('Invalid native service executable/data permissions')
    return extras


def checked_extras(value, rows):
    if value.get('vision') == 'linux-tesseract-bundled':
        from linux_ocr_runtime import bundled_records
        if value.get('host') != 'linux-x64': raise ValueError('Bundled OCR host mismatch')
        bundled_records(rows)
    elif rows:
        raise ValueError('Unexpected native service inventory')


def load_manifest(root):
    value = read_manifest(root)
    for row in manifest_records(value):
        verify_file(root, row)
        if (root / row['path']).stat().st_mode & 0o777 != row['mode']:
            raise ValueError('Native service permissions changed')
    if value.get('vision') == 'linux-tesseract-bundled':
        from linux_ocr_runtime import verify, PREFIX
        verify(root, root / PREFIX)
    return value


def service_command(root, role):
    name = binary_path(role)
    root = Path(root).resolve()
    load_manifest(root)
    return [str(root / name)]


def launch_spec(root, role):
    if role not in ('assistant', 'runner'):
        raise ValueError('Unknown service launch role')
    root = Path(root).resolve()
    folder = root / 'runtime/services'
    if folder.exists() or folder.is_symlink():
        manifest = load_manifest(root)
        if role=='assistant' and manifest.get('database_prepare')!=DATABASE_PREPARE_CONTRACT:
            raise ValueError('Native database preparation is missing; rebuild the complete client')
        command = [str(root/binary_path(role))]
        environment = {'AI_PHONE_OCR': str(root / binary_path('vision'))} if role == 'runner' else {}
        return {'argv': command, 'environment': environment, 'prebuilt': True}
    return {'argv': [str(root / 'target/debug' / ('ai-phone-' + role))], 'environment': {}, 'prebuilt': False}


def device_setup_spec(root):
    root = Path(root).resolve()
    folder = root / 'runtime/services'
    if folder.exists() or folder.is_symlink():
        manifest = load_manifest(root)
        if manifest.get('device_setup') != DEVICE_SETUP_CONTRACT:
            raise ValueError('Native device setup capability is missing; rebuild the complete client')
        return {'argv': [str(root / binary_path('assistant')), '--setup-devices'], 'prebuilt': True}
    # Source installs build the assistant during the preceding service phase.
    # Require its read-only capability before any activation; old binaries must not
    # interpret the new flag as a request to start a second server.
    from build_native_services import device_setup_receipt
    binary = root / 'target/debug/ai-phone-assistant'
    device_setup_receipt(binary)
    return {'argv': [str(binary), '--setup-devices'], 'prebuilt': False}


def supervisor_command(root):
    root=Path(root).resolve()
    if (root/'runtime/services').exists() or (root/'runtime/services').is_symlink():
        manifest=load_manifest(root)
        if manifest.get('service_supervision') != SERVICE_SUPERVISION_CONTRACT:
            raise ValueError('Native service supervision capability is missing; rebuild the complete client')
        return [str(root/binary_path('assistant'))]
    from build_native_services import contract_receipt
    binary=root/SOURCE_SUPERVISOR
    contract_receipt(binary,'--service-supervision-contract',SERVICE_SUPERVISION_CONTRACT)
    return [str(binary)]


def input_setup_command(root):
    root = Path(root).resolve()
    folder = root / 'runtime/services'
    if folder.exists() or folder.is_symlink():
        manifest = load_manifest(root)
        if manifest.get('phone_input_setup') != PHONE_INPUT_SETUP_CONTRACT:
            raise ValueError('Native phone input setup capability is missing; rebuild the complete client')
        return [str(root / binary_path('assistant'))]
    # This dedicated source helper rejects unknown flags without starting a
    # business server. Old assistant programs must never receive this new flag.
    from build_native_services import contract_receipt
    binary = root / SOURCE_SUPERVISOR
    contract_receipt(binary, '--phone-input-setup-contract', PHONE_INPUT_SETUP_CONTRACT)
    return [str(binary)]


def profile_command(root):
    return host_component_command(root,'host_profile','--host-profile-contract',HOST_PROFILE_CONTRACT)


def managed_engine_command(root):
    return host_component_command(root,'managed_engine','--managed-engine-contract',MANAGED_ENGINE_CONTRACT)


def host_component_command(root,field,flag,contract):
    root=Path(root).resolve()
    if (root/'runtime/services').exists() or (root/'runtime/services').is_symlink():
        manifest=load_manifest(root)
        if manifest.get(field)!=contract:
            raise ValueError('Native '+field.replace('_',' ')+' capability is missing; rebuild the complete client')
        return [str(root/binary_path('assistant'))]
    from build_native_services import contract_receipt
    binary=root/SOURCE_SUPERVISOR
    if not binary.exists():
        import subprocess
        subprocess.run(['cargo','build','-p','ai-phone-host-runtime','--bin','host-resources','--locked','--target-dir',str(root/'target')],cwd=root,check=True)
    contract_receipt(binary,flag,contract)
    return [str(binary)]


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--service', choices=ROLES, required=True)
    args = parser.parse_args()
    print(service_command(args.root, args.service)[0])
