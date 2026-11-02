"""Maintainer-only handoff from an inspected installer to native delivery volumes."""
import json
from pathlib import Path
import subprocess

ASSET_LIMIT=2*1024**3


def native(root,*arguments):
    result=subprocess.run(['cargo','run','--release','--locked','-p','ai-phone-host-runtime','--bin','package-volumes','--',
                           *map(str,arguments)],cwd=root,check=True,stdout=subprocess.PIPE,text=True)
    return json.loads(result.stdout)


def prepare(root,artifact):
    root=Path(root)
    name=artifact['file']
    if Path(name).name!=name or not artifact['delivery']['complete_device_bundle']:
        raise ValueError('Delivery requires a complete inspected installer')
    source=root/'dist'/name
    if source.stat().st_size!=artifact['size']:
        raise ValueError('Installer size changed before delivery')
    if artifact['size']<ASSET_LIMIT:
        return {'format':'single','file':name,'size':artifact['size'],'sha256':artifact['sha256']}
    folder=root/'dist'/(name+'.volumes')
    sealed=native(root,'split',source,folder,artifact['sha256'])
    manifest=folder/sealed['manifest']
    verified=native(root,'verify',manifest,sealed['manifest_sha256'])
    if verified.get('verified') is not True or verified['sha256']!=artifact['sha256'] or verified['size']!=artifact['size']:
        raise ValueError('Native volume verification differs from the installer')
    return {'format':'volumes','manifest':manifest.relative_to(root/'dist').as_posix(),
            'manifest_sha256':sealed['manifest_sha256'],'size':verified['size'],'sha256':verified['sha256'],
            'parts':json.loads(manifest.read_text())['parts']}
