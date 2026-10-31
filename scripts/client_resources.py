"""One inventory for native resource configuration and artifact inspection."""
import json
from pathlib import Path


def resource_map(root):
    root=Path(root);native=root/'apps/client/src-tauri'
    resources=dict(json.loads((native/'tauri.conf.json').read_text())['bundle']['resources'])
    integrity=native/'gen/host/integrity.json'
    if integrity.exists():
        expected=json.loads(integrity.read_text())
        if set(expected)-{'source.payload','entry.py','android.payload','images.payload','tools/manifest.json'}:
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
    return resources


def build_config(root):
    native=Path(root)/'apps/client/src-tauri'
    target=native/'gen/host/resources.json'
    expanded=resource_map(root)
    resources={source:destination for source,destination in expanded.items() if not source.startswith('gen/host/tools/')}
    if len(resources)!=len(expanded):resources['gen/host/tools']='host/tools'
    target.write_text(json.dumps({'bundle':{'resources':resources}},sort_keys=True)+'\n')
    return target


def public_build_digest(root,destination):
    if destination!='host/tools/node/bin/node':return None
    from installer_tools import read_manifest
    folder=Path(root)/'apps/client/src-tauri/gen/host/tools'
    host=read_manifest(folder)['host']
    pins=json.loads((Path(root)/'config/installer-tools.json').read_text())
    return pins['hosts'][host]['node'].get('public_build_paths_sha256')


def inspect_stream(source, name,public_build_digest=None):
    import hashlib
    import re
    from release_package import SECRET
    digest=hashlib.sha256();size=0;tail=b'';build_path=False
    while chunk:=source.read(1024**2):
        sample=tail+chunk
        if SECRET.search(sample) or SECRET.search(sample.replace(b'\x00',b'')):
            raise ValueError('Private material in bundle file: '+name)
        build_path=build_path or re.search(rb'/Users/[A-Za-z0-9._-]+/',sample) is not None
        digest.update(chunk);size+=len(chunk);tail=sample[-4096:]
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
