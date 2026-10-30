"""One inventory for native resource configuration and artifact inspection."""
import json
from pathlib import Path


def resource_map(root):
    root=Path(root);native=root/'apps/client/src-tauri'
    resources=dict(json.loads((native/'tauri.conf.json').read_text())['bundle']['resources'])
    integrity=native/'gen/host/integrity.json'
    if integrity.exists():
        expected=json.loads(integrity.read_text())
        if set(expected)-{'source.payload','entry.py','android.payload'}:
            raise ValueError('Unrecognized native resource')
        if 'android.payload' in expected:resources['gen/host/android.payload']='host/android.payload'
    return resources


def build_config(root):
    native=Path(root)/'apps/client/src-tauri'
    target=native/'gen/host/resources.json'
    target.write_text(json.dumps({'bundle':{'resources':resource_map(root)}},sort_keys=True)+'\n')
    return target


def inspect_stream(source, name):
    import hashlib
    import re
    from release_package import SECRET
    digest=hashlib.sha256();size=0;tail=b''
    while chunk:=source.read(1024**2):
        sample=tail+chunk
        if SECRET.search(sample) or SECRET.search(sample.replace(b'\x00',b'')) or re.search(rb'/Users/[A-Za-z0-9._-]+/',sample):
            raise ValueError('Private material in bundle file: '+name)
        digest.update(chunk);size+=len(chunk);tail=sample[-4096:]
    return {'size':size,'sha256':digest.hexdigest()}


def file_receipt(path):
    with Path(path).open('rb') as source:return inspect_stream(source,str(path.name))


def verify_compiled_inventory(binary, expected):
    missing={value.encode() for value in expected.values()};tail=b''
    with Path(binary).open('rb') as source:
        while chunk:=source.read(1024**2):
            sample=tail+chunk;missing={value for value in missing if value not in sample};tail=sample[-64:]
    if missing:
        raise ValueError('Compiled resources differ from the prepared payload; rebuild with stable inputs')
