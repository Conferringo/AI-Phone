"""Build host-matched native service artifacts; never installs or starts devices."""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tomllib
from host_platform import host_key
from release_package import select_files
from check_native_services import version_receipt, vision_receipt
from client_host_entry import checked_record, verify_file
from prebuilt_services import SOURCE_FIXTURE, DEVICE_SETUP_CONTRACT, SERVICE_SUPERVISION_CONTRACT, PHONE_INPUT_SETUP_CONTRACT

ROOT = Path(__file__).resolve().parent.parent
PACKAGES = {'assistant': 'assistant', 'runner': 'phone-runner', 'vision':'vision'}


def source_fingerprint(root):
    records = []
    for name, data, _ in select_files(root):
        if name in ('Cargo.toml', 'Cargo.lock', 'scripts/build_native_services.py', 'scripts/prebuilt_services.py', 'scripts/client_host.py', 'scripts/macos_target.py', 'apps/client/src-tauri/tauri.conf.json', 'config/ocr-models.json') or name.startswith(('crates/', 'packages/protocol/proto/', 'scripts/phone-vision/')):
            records.append([name, hashlib.sha256(data).hexdigest()])
    return hashlib.sha256(json.dumps(records, separators=(',', ':')).encode()).hexdigest()


def package_versions(root):
    return {role: tomllib.loads((root / 'crates' / folder / 'Cargo.toml').read_text())['package']
            for role, folder in PACKAGES.items()}


def build_environment(root):
    if platform.system() not in ('Darwin', 'Linux'):
        raise ValueError('Native services require a macOS/Linux build host')
    compiler = subprocess.check_output(['rustc', '--version'], text=True).strip()
    if compiler.split()[1] != '1.96.0':
        raise ValueError('Native service runtime notices require Rust 1.96.0')
    environment = {**os.environ, 'CARGO_BUILD_JOBS': '2', 'CARGO_INCREMENTAL': '0',
                   'CARGO_PROFILE_RELEASE_DEBUG': '0', 'CARGO_PROFILE_RELEASE_STRIP': 'debuginfo',
                   'CARGO_TARGET_DIR': str(root / 'target'), 'RUSTC': shutil.which('rustc')}
    environment.pop('CARGO_BUILD_TARGET', None)
    environment['CARGO_ENCODED_RUSTFLAGS'] = '--remap-path-prefix=' + str(Path.home()) + '=/build-user'
    return environment, compiler


def cached_artifact(root, role, record):
    expected = {'assistant': 'ai-phone-assistant', 'runner': 'ai-phone-runner', 'vision': 'ai-phone-vision'}
    if not isinstance(record, dict) or record.get('path') != 'target/release/' + expected[role]:
        return False
    try:
        row = checked_record({**record, 'mode': 0o755})
        verify_file(root, row)
    except (ValueError, OSError):
        return False
    return True


def read_build_receipt(path):
    if not path.is_file():
        return None
    try:
        value = json.loads(path.read_text())
    except (OSError, ValueError):
        return None
    if not isinstance(value, dict) or type(value.get('format')) is not int or value.get('format') != 1:
        return None
    checks = value.get('checks')
    if not isinstance(checks, dict) or checks.get('device_setup') != DEVICE_SETUP_CONTRACT:
        return None
    artifacts = value.get('artifacts')
    if not isinstance(artifacts, dict) or set(artifacts) != {'assistant', 'runner', 'vision'}:
        return None
    return value


def cached_build(root, fingerprint):
    value = read_build_receipt(root / 'target/release/native-service-build.json')
    if value is None or value.get('host') != host_key() or value.get('source_sha256') != fingerprint:
        return None
    if not all(cached_artifact(root, role, row) for role, row in value['artifacts'].items()):
        return None
    return value


def compile_rust(root, environment, versions):
    command = ['cargo', 'build', '--release', '--locked', '--message-format=json', '--features', 'ai-phone-vision/native']
    for package in versions.values():
        command.extend(['-p', package['name']])
    result = subprocess.run(command, cwd=root, env=environment, capture_output=True, text=True)
    if result.returncode:
        print(result.stderr[-12000:])
        raise ValueError('Native service compilation failed')
    executables = {row['target']['name']: Path(row['executable']).resolve() for line in result.stdout.splitlines()
                   if (row := json.loads(line)).get('reason') == 'compiler-artifact' and row.get('executable')}
    expected = {value['name']: (root / 'target/release' / value['name']).resolve() for value in versions.values()}
    if any(executables.get(name) != path for name, path in expected.items()):
        raise ValueError('Cargo returned an unexpected native executable')
    return {role: expected[package['name']] for role, package in versions.items()}


def device_setup_receipt(binary):
    return contract_receipt(binary,'--device-setup-contract',DEVICE_SETUP_CONTRACT)


def contract_receipt(binary,argument,expected):
    result = subprocess.run([str(binary), argument], capture_output=True, text=True, timeout=10, check=False)
    if result.returncode or result.stdout.strip() != expected or result.stderr:
        raise ValueError('Native execution capability qualification failed')
    return expected


def build(root=ROOT):
    root = Path(root).resolve()
    environment, compiler = build_environment(root)
    fingerprint = source_fingerprint(root)
    cached = cached_build(root, fingerprint)
    if cached:
        return cached
    if shutil.disk_usage(root).free < 4 * 1024**3:
        raise ValueError('At least 4 GiB free space is required for a new native service build')
    versions = package_versions(root)
    artifacts = compile_rust(root, environment, versions)
    checks = {role: version_receipt(path, versions[role]['name'], versions[role]['version'])
              for role, path in artifacts.items()}
    checks['device_setup'] = device_setup_receipt(artifacts['assistant'])
    checks['service_supervision'] = contract_receipt(artifacts['assistant'],'--service-supervision-contract',SERVICE_SUPERVISION_CONTRACT)
    checks['phone_input_setup'] = contract_receipt(artifacts['assistant'], '--phone-input-setup-contract', PHONE_INPUT_SETUP_CONTRACT)
    checks['vision_version'] = checks.pop('vision')
    checks['vision'] = vision_receipt(artifacts['vision'], root / SOURCE_FIXTURE, qualification=True)
    if platform.system() == 'Darwin':
        from macos_target import verify_targets
        checks['macos_minimum_versions'] = verify_targets(root, artifacts)
    if source_fingerprint(root) != fingerprint:
        raise ValueError('Native source changed during the build; artifact was not accepted')
    records = {role: {'path': path.relative_to(root).as_posix(), 'size': path.stat().st_size,
                      'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
               for role, path in artifacts.items()}
    receipt = {'format': 1, 'host': host_key(), 'source_sha256': fingerprint, 'compiler': compiler,
               'artifacts': records, 'checks': checks, 'developer_paths_remapped': True}
    (root / 'target/release/native-service-build.json').write_text(json.dumps(receipt, indent=2) + '\n')
    return receipt


if __name__ == '__main__':
    print(json.dumps(build()))
