"""Stage pinned vendor phone apps for offline installation, independent of host ABI."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
from apk_review import inspect_apk
from artifact_io import digest
from client_host_entry import checked_record,verify_file

ABIS=('arm64-v8a','x86_64')
PACKAGE='com.github.metacubex.clash.meta'
PREFIX='runtime/guest-apps/'
SOURCE=('config/clash-apps.json','scripts/prebuilt_guest_apps.py','scripts/apk_review.py')
MAX_APK=64*1024**2
RESERVE=4*1024**3


def source_fingerprint(root):
    rows=[[name,digest(Path(root)/name)] for name in SOURCE]
    return hashlib.sha256(json.dumps(rows,separators=(',',':')).encode()).hexdigest()


def checked_pin(value):
    if not isinstance(value,dict):raise ValueError('Missing guest application pin')
    for key in ('sha256','signer_sha256'):
        if re.fullmatch('[0-9a-f]{64}',str(value.get(key))) is None:raise ValueError('Invalid guest application identity')
    size=value.get('size_bytes');name=value.get('file')
    if type(size) is not int or not 0<size<=MAX_APK:raise ValueError('Invalid guest application size')
    if not isinstance(name,str) or re.fullmatch(r'[A-Za-z0-9._-]+\.apk',name) is None:raise ValueError('Invalid guest application filename')
    if re.fullmatch(r'v\d+\.\d+\.\d+',str(value.get('version'))) is None:raise ValueError('Invalid guest application version')
    return value


def pins(root):
    value=json.loads((Path(root)/'config/clash-apps.json').read_text(encoding='utf-8'))
    if not isinstance(value,dict) or not isinstance(value.get('alternatives'),dict):raise ValueError('Invalid guest application pins')
    return {'arm64-v8a':checked_pin(value.get('android')),'x86_64':checked_pin(value.get('alternatives',{}).get('android-x86_64'))}


def app_identity(pin):
    return {'package':PACKAGE,'version':pin['version'],'signer_sha256':pin['signer_sha256']}


def records(metadata):
    return [{'path':PREFIX+'clash-'+abi+'.apk','size':metadata[abi]['size_bytes'],'sha256':metadata[abi]['sha256'],'mode':0o644} for abi in ABIS]


def read_manifest(folder):
    if folder.is_symlink():raise ValueError('Guest application component cannot be an alias')
    path=folder/'manifest.json'
    if path.is_symlink() or path.stat().st_size>65536:raise ValueError('Invalid guest application manifest')
    value=json.loads(path.read_text(encoding='utf-8'))
    if not isinstance(value,dict) or set(value)!={'format','source_sha256','apps','files'}:raise ValueError('Invalid guest application manifest')
    if type(value['format']) is not int or value['format']!=1:raise ValueError('Invalid guest application format')
    return value


def checked_inventory(value,metadata):
    if value['apps']!={abi:app_identity(pin) for abi,pin in metadata.items()}:raise ValueError('Guest application identity differs from pinned source')
    if not isinstance(value['files'],list) or len(value['files'])!=len(ABIS):raise ValueError('Incomplete guest application inventory')
    if sorted(map(checked_record,value['files']),key=lambda r:r['path'])!=records(metadata):raise ValueError('Guest application inventory differs from pinned source')
    return value


def component_files(folder):
    expected={'manifest.json',*['clash-'+abi+'.apk' for abi in ABIS]}
    if {p.name for p in folder.iterdir()}!=expected:raise ValueError('Unexpected guest application files')
    for name in expected:
        path=folder/name
        if path.is_symlink() or not path.is_file():raise ValueError('Invalid guest application resource')


def manifest(root,folder):
    value=read_manifest(folder)
    if value['source_sha256']!=source_fingerprint(root):raise ValueError('Guest application source changed; rebuild before packaging')
    checked_inventory(value,pins(root));component_files(folder)
    return value


def verify(root,folder):
    value=manifest(Path(root),Path(folder))
    for row in value['files']:verify_file(Path(folder),{**row,'path':Path(row['path']).name})
    return value


def selected(root,abi):
    if abi not in ABIS:raise ValueError('Unsupported guest application ABI')
    root=Path(root).resolve();folder=root/'runtime/guest-apps'
    if not folder.exists() and not folder.is_symlink():return None
    manifest(root,folder)
    return folder/('clash-'+abi+'.apk')


def environment(root,abi):
    # Discovery is on the adb environment path. The installation consumer hashes
    # the selected APK; do not rehash 100MB of archives for every device action.
    path=selected(root,abi)
    return {'AI_PHONE_CLASH_ANDROID_APK':str(path)} if path is not None else {}


def application(root,abi):
    path=selected(root,abi)
    if path is not None:verify_file(path.parent,{'path':path.name,'mode':0o644,'size':pins(root)[abi]['size_bytes'],'sha256':pins(root)[abi]['sha256']})
    return path


def check_apk(path,abi,pin,tools,env):
    if path.is_symlink() or path.stat().st_size!=pin['size_bytes'] or digest(path)!=pin['sha256']:raise ValueError('Guest APK differs from pinned bytes')
    inspect_apk(path,max_entries=10000,max_unpacked=256*1024**2)
    output=subprocess.check_output([str(tools/'apksigner'),'verify','--print-certs',str(path)],env=env,text=True,encoding='utf-8')
    if re.findall(r'Signer #\d+ certificate SHA-256 digest: ([0-9a-f]{64})',output)!=[pin['signer_sha256']]:raise ValueError('Unexpected guest APK signer')
    metadata=subprocess.check_output([str(tools/'aapt2'),'dump','badging',str(path)],env=env,text=True,encoding='utf-8')
    if not metadata.startswith("package: name='"+PACKAGE+"'") or "versionName='"+pin['version'][1:]+".Meta'" not in metadata:raise ValueError('Unexpected guest APK package or version')
    if re.findall(r"^native-code: '([^']+)'$",metadata,re.M)!=[abi]:raise ValueError('Unexpected guest APK ABI')


def build(root,tools,env):
    root=Path(root).resolve();destination=root/'.tools/guest-apps'
    if destination.exists() or destination.is_symlink():return verify(root,destination)
    metadata=pins(root);identity=source_fingerprint(root)
    if shutil.disk_usage(root).free<RESERVE+sum(pin['size_bytes'] for pin in metadata.values()):raise ValueError('Keep 4 GiB free while staging guest applications')
    destination.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.guest-apps-',dir=destination.parent) as temporary:
        folder=Path(temporary)/'component';folder.mkdir()
        for abi,pin in metadata.items():
            source=root/'.tools/clash-apps/android'/pin['file'];target=folder/('clash-'+abi+'.apk')
            if source.is_symlink() or not source.is_file():raise ValueError('Prepare pinned vendor APKs before building')
            shutil.copyfile(source,target);target.chmod(0o644);check_apk(target,abi,pin,tools,env)
        value={'format':1,'source_sha256':identity,'apps':{abi:app_identity(pin) for abi,pin in metadata.items()},'files':records(metadata)}
        (folder/'manifest.json').write_text(json.dumps(value,sort_keys=True)+'\n',encoding='utf-8')
        verify(root,folder);folder.rename(destination)
    return value


def prepare_apps(root):
    folder=Path(root)/'.tools/guest-apps';verify(root,folder)
    return [(PREFIX+path.name,path.read_bytes(),0o644) for path in sorted(folder.iterdir())]


if __name__=='__main__':
    import os
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--build',action='store_true');parser.add_argument('--tools',type=Path);args=parser.parse_args()
    if not args.build or not args.tools:parser.error('Maintainer build requires --build --tools SDK/build-tools/version')
    result=build(args.root,args.tools,dict(os.environ))
    print(json.dumps({'files':len(result['files']),'bytes':sum(row['size'] for row in result['files']),'source_sha256':result['source_sha256']}))
