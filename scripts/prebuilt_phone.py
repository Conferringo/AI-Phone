"""Prebuilt phone control APKs. Source builds retain their separate compiler path."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile
from client_host_entry import checked_record, verify_file
from release_package import select_files
from apk_review import inspect_apk

APPS={'input':'dev.aiphone.input','home':'dev.aiphone.home'}
PREFIX='runtime/phone-apps/'
PROBE='notification-access.jar'
PROBE_SOURCE='scripts/android-input-probe/NotificationAccess.java'
MAX_PROBE=64*1024


def source_fingerprint(root):
    records=[[name,hashlib.sha256(data).hexdigest()] for name,data,_ in select_files(root)
             if name.startswith(('apps/android-input/','apps/android-home/')) or name in ('scripts/prebuilt_phone.py','scripts/apk_review.py',PROBE_SOURCE)]
    return hashlib.sha256(json.dumps(records,separators=(',',':')).encode()).hexdigest()


def manifest(folder):
    path=folder/'manifest.json'
    if path.is_symlink() or path.stat().st_size>65536:raise ValueError('Invalid phone application manifest')
    value=json.loads(path.read_text())
    if not isinstance(value,dict) or type(value.get('format')) is not int or value['format']!=1:
        raise ValueError('Invalid phone application manifest')
    if not isinstance(value.get('source_sha256'),str) or re.fullmatch('[0-9a-f]{64}',value['source_sha256']) is None:
        raise ValueError('Invalid phone application source')
    validate_apps(value.get('apps'))
    validate_records(value.get('files'))
    return value


def validate_apps(apps):
    if not isinstance(apps,dict) or set(apps)!=set(APPS):raise ValueError('Incomplete phone applications')
    for role,package in APPS.items():
        row=apps[role]
        if not isinstance(row,dict) or row.get('package')!=package or re.fullmatch('[0-9a-f]{64}',str(row.get('signer_sha256'))) is None:
            raise ValueError('Invalid phone application identity')


def validate_records(rows):
    if not isinstance(rows,list) or len(rows)!=len(APPS)+1:raise ValueError('Incomplete phone application inventory')
    expected={PREFIX+role+'.apk' for role in APPS}|{PREFIX+PROBE}
    records=[checked_record(row) for row in rows]
    if {row['path'] for row in records}!=expected or any(row['mode']!=0o644 or not 0<row['size']<=16*1024**2 for row in records):
        raise ValueError('Invalid phone application inventory')
    if any(row['path']==PREFIX+PROBE and row['size']>MAX_PROBE for row in records):
        raise ValueError('Invalid notification probe inventory')


def verify(root,folder):
    if folder.is_symlink():raise ValueError('Phone applications cannot be a symlink')
    value=manifest(folder)
    if value['source_sha256']!=source_fingerprint(root):raise ValueError('Phone application source changed; rebuild before packaging')
    if {p.name for p in folder.iterdir()}!={'manifest.json',PROBE,*[role+'.apk' for role in APPS]}:
        raise ValueError('Unexpected phone application files')
    for row in value['files']:
        verify_file(folder,{**row,'path':Path(row['path']).name})
    inspect_probe(folder/PROBE)
    return value


def application(root,role):
    if role not in APPS:raise ValueError('Unknown phone application')
    root=Path(root).resolve();folder=root/'runtime/phone-apps'
    if not folder.exists() and not folder.is_symlink():return None
    verify(root,folder)
    return folder/(role+'.apk')


def available(root):
    return application(root,'input') is not None


def inspect_probe(path):
    inspect_apk(path,max_entries=1,max_unpacked=MAX_PROBE)
    with zipfile.ZipFile(path) as archive:
        if archive.namelist()!=['classes.dex']:raise ValueError('Invalid notification probe contents')


def notification_helper(root):
    root=Path(root).resolve();folder=root/'runtime/phone-apps'
    if folder.exists() or folder.is_symlink():
        verify(root,folder)
        return folder/PROBE
    # Match installer_apk's source-only branch; a present invalid bundle never compiles.
    return compile_probe(root,root/'data/android',dict(os.environ))


def prepare_apps(root):
    root=Path(root).resolve();folder=root/'.tools/phone-apps';verify(root,folder)
    for role in APPS:inspect_apk(folder/(role+'.apk'))
    return [(PREFIX+path.name,path.read_bytes(),0o644) for path in sorted(folder.iterdir())]


def check_apk(path,role,tools,env):
    inspect_apk(path)
    result=subprocess.check_output([str(tools/'apksigner'),'verify','--print-certs',str(path)],env=env,text=True)
    signers=re.findall(r'Signer #\d+ certificate SHA-256 digest: ([0-9a-f]{64})',result)
    if len(signers)!=1:raise ValueError('Phone APK needs one verified signer')
    metadata=subprocess.check_output([str(tools/'aapt2'),'dump','badging',str(path)],env=env,text=True)
    if not metadata.startswith("package: name='"+APPS[role]+"'"):raise ValueError('Unexpected phone APK package')
    return {'package':APPS[role],'signer_sha256':signers[0]}


def compile_app(work,role,keys,env):
    output=work/'data/android'/role;output.mkdir(parents=True)
    key=keys/(role+'.jks')
    if key.exists():shutil.copyfile(key,output/'debug.jks')
    subprocess.run([sys.executable,str(work/'apps'/('android-'+role)/'build.py')],env=env,check=True,stdout=sys.stderr)
    if not key.exists():
        shutil.copyfile(output/'debug.jks',key);key.chmod(0o600)
    return output/('ai-phone-'+role+'.apk')


def compile_probe(root,work,env):
    output=work/'notification-probe';classes=output/'classes';dex=output/'dex'
    classes.mkdir(parents=True,exist_ok=True);dex.mkdir(exist_ok=True)
    sdk=Path(env['ANDROID_HOME']);jar=sdk/'platforms/android-35/android.jar'
    subprocess.run([str(Path(env['JAVA_HOME'])/'bin/javac'),'--release','8','-classpath',str(jar),
        '-d',str(classes),str(root/PROBE_SOURCE)],env=env,check=True,stdout=sys.stderr)
    subprocess.run([str(sdk/'build-tools/35.0.0/d8'),'--lib',str(jar),'--min-api','26','--output',str(dex),
        str(classes/'dev/aiphone/input/probe/NotificationAccess.class')],env=env,check=True,stdout=sys.stderr)
    target=output/PROBE
    with zipfile.ZipFile(target,'w',compression=zipfile.ZIP_DEFLATED) as archive:
        archive.write(dex/'classes.dex','classes.dex')
    inspect_probe(target)
    return target


def build(root,env):
    root=Path(root).resolve();destination=root/'.tools/phone-apps'
    if destination.exists():
        verify(root,destination);return {'reused':True,'source_sha256':source_fingerprint(root)}
    destination.parent.mkdir(exist_ok=True)
    if shutil.disk_usage(destination.parent).free<4*1024**3+32*1024**2:raise ValueError('Keep 4 GiB free while building phone applications')
    keys=root/'.tools/phone-app-signing';keys.mkdir(exist_ok=True,mode=0o700)
    with tempfile.TemporaryDirectory(prefix='.phone-app-build-',dir=destination.parent) as temporary:
        work=Path(temporary);component=work/'component';component.mkdir();apps={};records=[]
        for role in APPS:
            shutil.copytree(root/'apps'/('android-'+role),work/'apps'/('android-'+role))
            path=compile_app(work,role,keys,env)
            apps[role]=check_apk(path,role,Path(env['ANDROID_HOME'])/'build-tools/35.0.0',env)
            target=component/(role+'.apk');shutil.copyfile(path,target);target.chmod(0o644)
            records.append({'path':PREFIX+target.name,'size':target.stat().st_size,'sha256':hashlib.sha256(target.read_bytes()).hexdigest(),'mode':0o644})
        target=component/PROBE;shutil.copyfile(compile_probe(root,work,env),target);target.chmod(0o644)
        records.append({'path':PREFIX+PROBE,'size':target.stat().st_size,'sha256':hashlib.sha256(target.read_bytes()).hexdigest(),'mode':0o644})
        value={'format':1,'source_sha256':source_fingerprint(root),'apps':apps,'files':records}
        (component/'manifest.json').write_text(json.dumps(value,sort_keys=True)+'\n');verify(root,component);component.rename(destination)
    return {'source_sha256':value['source_sha256'],'apps':apps,'files':records}


def installer_apk(root,role):
    path=application(root,role)
    if path is not None:return path
    subprocess.run([sys.executable,str(root/'apps'/('android-'+role)/'build.py')],check=True,stdout=sys.stderr)
    return root/'data/android'/role/('ai-phone-'+role+'.apk')


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--build',action='store_true');parser.add_argument('--app',choices=APPS)
    parser.add_argument('--notification-probe',action='store_true');args=parser.parse_args()
    if args.build:print(json.dumps(build(args.root,dict(os.environ))))
    elif args.app:print(installer_apk(args.root,args.app))
    elif args.notification_probe:print(notification_helper(args.root))
    else:parser.error('Choose a build or application')
