"""Inspect unpacked native payloads and record honest local-preview manifests."""
import hashlib
import json
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile
import os
from release_package import SECRET

ROOT = Path(__file__).resolve().parent.parent


def notice_manifest(root=ROOT):
    inventory=json.loads((root/'docs/public/dependency-licenses.json').read_text())
    missing=inventory.get('native_client_missing_notice_files')
    if missing!=[]:
        raise ValueError('Native dependency notice evidence is incomplete')
    name='native-client-licenses.txt'
    return {'rust_missing_files':missing,'texts':{name:hashlib.sha256((root/'docs/public'/name).read_bytes()).hexdigest()}}


def inspect_payload(files, root=ROOT):
    import io
    from client_resources import resource_map, file_receipt, inspect_stream,public_build_digest
    native=root/'apps/client/src-tauri'
    resources=resource_map(root)
    public={destination:public_build_digest(root,destination) for destination in resources.values()}
    legal={destination:file_receipt(native/source,public[destination]) for source,destination in resources.items()}
    seen=set();result=[]
    for name,data in sorted(files.items()):
        relative=next((relative for relative in legal if name.endswith('/'+relative) or name==relative),None)
        if relative is None:
            for group,label in [('tools','tool'),('wsl','WSL')]:
                if '/host/'+group+'/' in ('/'+name).casefold():raise ValueError('Unexpected installer '+label+' resource: '+name)
        approved=public.get(relative)
        receipt=file_receipt(data,approved) if isinstance(data,Path) else inspect_stream(io.BytesIO(data),name,approved)
        if relative is not None:
            if receipt!=legal[relative]:raise ValueError('Stale legal resource: '+name)
            seen.add(relative)
        result.append({'path':name,**receipt})
    if seen!=set(legal):raise ValueError('Missing legal resources: '+str(sorted(set(legal)-seen)))
    return result




def unpacked_files(base):
    base=base.resolve()
    files={}
    for path in base.rglob('*'):
        if path.is_symlink() and not path.resolve().is_relative_to(base):
            raise ValueError('Unpacked installer link leaves its scratch directory: '+path.relative_to(base).as_posix())
        if path.is_file():files[path.relative_to(base).as_posix()]=path
    return files


def inspect_linux_program(base,architecture):
    path=base/'usr/bin/ai-phone-client'
    with path.open('rb') as program:header=program.read(20)
    if len(header)<20 or header[:6]!=b'\x7fELF\x02\x01':raise ValueError('Invalid Linux client executable')
    machine={'amd64':62,'arm64':183}.get(architecture)
    if machine is None or struct.unpack_from('<H',header,18)[0]!=machine:
        raise ValueError('Unexpected Linux executable architecture')


def inspect_appimage(path,architecture,root=ROOT):
    # These are locally built artifacts. Extraction executes only the image's
    # native extraction mode, without FUSE, a display, or launching the client.
    with tempfile.TemporaryDirectory(prefix='ai-phone-appimage-') as temporary:
        subprocess.run([str(path.resolve()),'--appimage-extract'],cwd=temporary,check=True,stdout=subprocess.DEVNULL)
        base=Path(temporary)/'squashfs-root'
        files=unpacked_files(base)
        inspect_linux_program(base,architecture)
        return {'architecture':architecture,'files':inspect_payload(files,root)}


def inspect_deb(path, root=ROOT):
    with tempfile.TemporaryDirectory(prefix='ai-phone-deb-') as temp:
        subprocess.run(['dpkg-deb', '-x', str(path), temp], check=True)
        base = Path(temp)
        files = unpacked_files(base)
        architecture = subprocess.check_output(['dpkg-deb', '-f', str(path), 'Architecture'], text=True).strip()
        inspect_linux_program(base,architecture)
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
        'signing': signing, 'runtime_acceptance': runtime,
        'dependencies_license_review_complete': False, 'remote_upload': False,
        'third_party_notices': notice_manifest(root), 'source_commit': os.environ.get('GITHUB_SHA',''),
        'source_locks': {name: hashlib.sha256((root/name).read_bytes()).hexdigest()
                         for name in ['bun.lock', 'apps/client/src-tauri/Cargo.lock']},
        'source_payload_included': True,
        'local_bootstrap': {'implemented_on': sorted(json.loads((root/'config/host-bootstrap.json').read_text())['hosts']), 'fresh_install_acceptance': 'pending', 'user_state_bundled': False},
        'payload': inspection}
    from client_delivery import delivery_metadata,installer_metadata
    manifest.update(delivery_metadata(payload_components(root),os_name+'-'+arch))
    if os_name=='windows' and manifest['complete_device_bundle']:
        manifest['local_bootstrap']['implemented_on'].append('windows-x64')
    manifest['installer_delivery']=installer_metadata(path,os_name+'-'+arch)
    path.with_suffix(path.suffix+'.manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    return manifest
