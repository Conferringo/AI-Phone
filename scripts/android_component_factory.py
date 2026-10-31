"""Build the matched SDK/image component from pinned stable factory archives."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

from android_runtime import ABIS,build as build_runtime
from android_sdk_archive import unpack_sdk
from client_host import download
from host_platform import host_key


def policy(root):
    value=json.loads((Path(root)/'config/android-factory.json').read_text())
    host=host_key();selected=value.get('hosts',{}).get(host)
    if value.get('format')!=1 or value.get('api')!=35 or value.get('license_id')!='android-sdk-license' or not isinstance(selected,dict):
        raise ValueError('No matched Android factory policy')
    if selected.get('abi')!=ABIS.get(host) or selected['abi'] not in value.get('images',{}):raise ValueError('Factory guest ABI mismatch')
    archives=selected.get('archives',{})
    if set(archives)!= {'emulator','platform-tools'} or any(p.get('channel')!='stable' for p in archives.values()):
        raise ValueError('Factory requires stable SDK archives')
    return {**archives,'system-image':value['images'][selected['abi']]}


def archives_for(pins,work):
    archives={}
    for role,pin in pins.items():
        path=download(pin,work/(role+'.zip'))
        if path.stat().st_size!=pin['size_bytes']:raise ValueError('Android factory archive size differs')
        archives[role]=path
    return archives


def checked_revision(sdk,role,pin):
    text=(sdk/role/'source.properties').read_text(encoding='utf-8')
    rows=[line.split('=',1)[1].strip() for line in text.splitlines() if '=' in line and line.split('=',1)[0].strip()=='Pkg.Revision']
    if rows!=[pin['version']]:raise ValueError('SDK component revision differs from its stable pin')


def build(root,work):
    root=Path(root).resolve();pins=policy(root);work=Path(work);work.mkdir(parents=True)
    archives=archives_for(pins,work);output=work/'android.payload'
    with tempfile.TemporaryDirectory(prefix='.sdk-',dir=work) as temporary:
        folder=Path(temporary);sdk=folder/'sdk';sdk.mkdir()
        for role in ('emulator','platform-tools'):
            unpack_sdk(archives[role],folder/role,role);(folder/role/role).rename(sdk/role);checked_revision(sdk,role,pins[role])
        result=build_runtime(sdk,archives['system-image'],output,pins['system-image']['sha256'])
    return output,result


def qualify(root,payload,home):
    from android_runtime import stage,verify_sdk
    pins=policy(root);home=Path(home).resolve();sdk=stage(payload,home);manifest=verify_sdk(sdk);suffix='.exe' if host_key()=='windows-x64' else ''
    profile=home/'qualification';profile.mkdir()
    environment={**os.environ,'ANDROID_HOME':str(sdk),'ANDROID_SDK_ROOT':str(sdk),'ANDROID_USER_HOME':str(profile/'android'),
                 'ANDROID_EMULATOR_HOME':str(profile/'emulator'),'ANDROID_AVD_HOME':str(profile/'avd'),'HOME':str(profile),
                 'USERPROFILE':str(profile),'LOCALAPPDATA':str(profile/'local'),'APPDATA':str(profile/'roaming')}
    versions={}
    for role,program,argument in [('emulator','emulator','-version'),('platform-tools','adb','version')]:
        result=subprocess.run([sdk/role/(program+suffix),argument],cwd=profile,env=environment,capture_output=True,text=True,timeout=15,check=True)
        if pins[role]['version'] not in result.stdout:raise ValueError('Android runtime program version differs from its pin')
        versions[role]=result.stdout.splitlines()[:3]
    return {'host':manifest['host'],'abi':manifest['abi'],'files':len(manifest['files']),'programs':versions,'guest_boot':'not-executed'}


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--work',type=Path,required=True);parser.add_argument('--receipt',type=Path)
    parser.add_argument('--verify-programs',action='store_true');args=parser.parse_args();output,result=build(args.root,args.work)
    if args.verify_programs:result['qualification']=qualify(args.root,output,args.work/'installation')
    if args.receipt:
        args.receipt.parent.mkdir(parents=True,exist_ok=True);args.receipt.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({'output':str(output),**result}))
