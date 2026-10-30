"""Inspect unpacked native payloads and record honest local-preview manifests."""
import hashlib
import json
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile
import tomllib
import zipfile
import os
from release_package import SECRET

ROOT = Path(__file__).resolve().parent.parent


def notice_manifest(root=ROOT):
    inventory=json.loads((root/'docs/public/dependency-licenses.json').read_text())
    missing=inventory.get('native_client_missing_notice_files')
    if missing!=[]:
        raise ValueError('Native dependency notice evidence is incomplete')
    android=json.loads((root/'config/android-dependency-notices.json').read_text())
    if not android['packages']:
        raise ValueError('Android dependency notices are empty')
    texts={name:hashlib.sha256((root/'docs/public'/name).read_bytes()).hexdigest()
           for name in ['native-client-licenses.txt','android-client-licenses.txt']}
    return {'rust_missing_files':missing,'android_runtime_modules':len(android['packages']),'texts':texts}


def inspect_payload(files, root=ROOT):
    import io
    from client_resources import resource_map, file_receipt, inspect_stream
    native=root/'apps/client/src-tauri'
    legal={destination:file_receipt(native/source) for source,destination in resource_map(root).items()}
    seen=set();result=[]
    for name,data in sorted(files.items()):
        receipt=file_receipt(data) if isinstance(data,Path) else inspect_stream(io.BytesIO(data),name)
        for relative,expected in legal.items():
            if name.endswith('/'+relative) or name==relative:
                if receipt!=expected:raise ValueError('Stale legal resource: '+name)
                seen.add(relative)
        result.append({'path':name,**receipt})
    if seen!=set(legal):raise ValueError('Missing legal resources: '+str(sorted(set(legal)-seen)))
    return result


def inspect_apk(path, root=ROOT):
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)): raise ValueError('Duplicate APK entries')
        files = {name: archive.read(name) for name in names if not name.endswith('/')}
    libraries = [name for name in files if name.endswith('.so')]
    if not libraries or any(not name.startswith('lib/arm64-v8a/') for name in libraries):
        raise ValueError('Unexpected Android architecture')
    manifest = root/'apps/client/src-tauri/Cargo.toml'
    if manifest.exists():
        expected = 'lib/arm64-v8a/lib'+tomllib.loads(manifest.read_text())['lib']['name']+'.so'
        if expected not in libraries or any('libai_phone_' in name and name != expected for name in libraries):
            raise ValueError('Stale or missing Android native library')
    for name in libraries:
        data = files[name]
        if data[:5] != b'\x7fELF\x02' or struct.unpack_from('<H', data, 18)[0] != 183:
            raise ValueError('Invalid ARM64 Android library')
    return inspect_payload(files, root)


def inspect_deb(path, root=ROOT):
    with tempfile.TemporaryDirectory(prefix='ai-phone-deb-') as temp:
        subprocess.run(['dpkg-deb', '-x', str(path), temp], check=True)
        base = Path(temp)
        files = {p.relative_to(base).as_posix(): p for p in base.rglob('*') if p.is_file()}
        architecture = subprocess.check_output(['dpkg-deb', '-f', str(path), 'Architecture'], text=True).strip()
        if architecture not in ('arm64', 'amd64'): raise ValueError('Unexpected DEB architecture')
        return {'architecture': architecture, 'files': inspect_payload(files, root)}


def inspect_windows(path, root=ROOT):
    # Inspect the expanded NSIS payload, not merely compressed installer bytes.
    seven = shutil.which('7z') or shutil.which('7zz')
    if not seven: raise ValueError('Install 7-Zip to inspect the Windows installer payload')
    with tempfile.TemporaryDirectory(prefix='ai-phone-nsis-') as temp:
        subprocess.run([seven, 'x', '-y', '-o'+temp, str(path)], check=True, stdout=subprocess.DEVNULL)
        base = Path(temp)
        files = {p.relative_to(base).as_posix(): p for p in base.rglob('*') if p.is_file()}
        payload = next((data for name, data in files.items() if name.endswith('ai-phone-client.exe')), None)
        payload=payload.read_bytes() if payload is not None else None
        if not payload or payload[:2] != b'MZ': raise ValueError('Missing Windows executable')
        pe = struct.unpack_from('<I', payload, 60)[0]
        if payload[pe:pe+4] != b'PE\0\0' or struct.unpack_from('<H', payload, pe+4)[0] != 0x8664:
            raise ValueError('Unexpected Windows executable architecture')
        return inspect_payload(files, root)


def record(path, os_name, arch, inspection, root=ROOT, signing='unsigned', runtime='pending'):
    from prepare_client_host import payload_components
    with path.open('rb') as source:digest=hashlib.file_digest(source,'sha256').hexdigest()
    path.with_suffix(path.suffix+'.sha256').write_text(digest+'  '+path.name+'\n')
    manifest = {'version': json.loads((root/'config/release-policy.json').read_text())['version'],
        'kind': 'native-client-preview', 'os': os_name, 'arch': arch, 'sha256': digest,
        'signing': signing, 'runtime_acceptance': runtime, 'server_included': False,
        'dependencies_license_review_complete': False, 'remote_upload': False,
        'third_party_notices': notice_manifest(root), 'source_commit': os.environ.get('GITHUB_SHA',''),
        'source_locks': {name: hashlib.sha256((root/name).read_bytes()).hexdigest()
                         for name in ['bun.lock', 'apps/client/src-tauri/Cargo.lock']},
        'source_payload_included': True,
        'prebuilt_components': payload_components(root), 'complete_device_bundle': False,
        'local_bootstrap': {'implemented_on': sorted(json.loads((root/'config/host-bootstrap.json').read_text())['hosts']), 'fresh_install_acceptance': 'pending', 'user_state_bundled': False},
        'payload': inspection}
    path.with_suffix(path.suffix+'.manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    return manifest
