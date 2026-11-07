"""One inventory for native resource configuration and artifact inspection."""
import json
from pathlib import Path
from public_vectors import WINDOW,mask_public_vectors


def resource_map(root):
    root=Path(root);native=root/'apps/client/src-tauri'
    resources=dict(json.loads((native/'tauri.conf.json').read_text())['bundle']['resources'])
    integrity=native/'gen/host/integrity.json'
    if integrity.exists():
        expected=json.loads(integrity.read_text())
        if set(expected)-{'source.payload','android.payload','images.payload','tools/manifest.json','wsl/manifest.json'}:
            raise ValueError('Unrecognized native resource')
        for name in ('android.payload','images.payload'):
            if name in expected:resources['gen/host/'+name]='host/'+name
        if 'tools/manifest.json' in expected:
            from installer_tools import verify
            from artifact_io import digest
            folder=native/'gen/host/tools'
            if digest(folder/'manifest.json')!=expected['tools/manifest.json']:raise ValueError('Installer tools changed before packaging')
            manifest=verify(folder)
            for name in ['manifest.json',*[row['path'] for row in manifest['files']]]:resources['gen/host/tools/'+name]='host/tools/'+name
        if 'wsl/manifest.json' in expected:
            from wsl_resources import verify
            verify(native/'gen/host/wsl',expected['wsl/manifest.json'])
            for name in ['manifest.json','rootfs.tar.gz']:resources['gen/host/wsl/'+name]='host/wsl/'+name
    return resources


def build_config(root):
    native=Path(root)/'apps/client/src-tauri'
    target=native/'gen/host/resources.json'
    expanded=resource_map(root)
    resources=dict(expanded)
    for group in ['tools','wsl']:
        members=[name for name in resources if name.startswith('gen/host/'+group+'/')]
        for name in members:resources.pop(name)
        if members:resources['gen/host/'+group]='host/'+group
    target.write_text(json.dumps({'bundle':{'resources':resources}},sort_keys=True)+'\n')
    return target


def public_build_digest(root,destination):
    if destination!='host/tools/node/bin/node':return None
    from installer_tools import read_manifest
    folder=Path(root)/'apps/client/src-tauri/gen/host/tools'
    host=read_manifest(folder)['host']
    pins=json.loads((Path(root)/'config/installer-tools.json').read_text())
    return pins['hosts'][host]['node'].get('public_build_paths_sha256')


def private_content(sample,boundary):
    from release_package import SECRET
    clean=mask_public_vectors(sample)
    variants=[(clean,boundary),(clean.replace(b'\x00',b''),boundary-clean.count(b'\x00',0,boundary))]
    return any((match:=SECRET.search(data)) is not None and match.start()<end for data,end in variants)


def inspect_stream(source, name,public_build_digest=None):
    import hashlib
    import re
    digest=hashlib.sha256();size=0;tail=b'';build_path=False
    while chunk:=source.read(1024**2):
        sample=tail+chunk
        # Hold a complete fixture-sized suffix; never reject a partial public
        # string before its hash and terminating NUL can be checked.
        if private_content(sample,max(0,len(sample)-WINDOW)):
            raise ValueError('Private material in bundle file: '+name)
        build_path=build_path or re.search(rb'/Users/[A-Za-z0-9._-]+/',sample) is not None
        digest.update(chunk);size+=len(chunk);tail=sample[-WINDOW:]
    if private_content(tail,len(tail)):
        raise ValueError('Private material in bundle file: '+name)
    identity=digest.hexdigest()
    if (public_build_digest is not None and identity!=public_build_digest) or (build_path and identity!=public_build_digest):
        raise ValueError('Private material in bundle file: '+name)
    return {'size':size,'sha256':identity}


def file_receipt(path,public_build_digest=None):
    with Path(path).open('rb') as source:return inspect_stream(source,str(path.name),public_build_digest)


def verify_compiled_inventory(binary, expected):
    missing={value.encode() for value in expected.values()};tail=b''
    with Path(binary).open('rb') as source:
        while chunk:=source.read(1024**2):
            sample=tail+chunk;missing={value for value in missing if value not in sample};tail=sample[-64:]
    if missing:
        raise ValueError('Compiled resources differ from the prepared payload; rebuild with stable inputs')
