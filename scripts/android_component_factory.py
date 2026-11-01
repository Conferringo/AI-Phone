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
from android_factory_stock import macos_policy,stock_identity,reserve,extraction_reserve,archive_image,sdk_programs,built_receipt,image_properties


def policy(root):
    value=json.loads((Path(root)/'config/android-factory.json').read_text())
    host=host_key();selected=value.get('hosts',{}).get(host)
    if value.get('format')!=1 or value.get('api')!=35 or value.get('license_id')!='android-sdk-license' or not isinstance(selected,dict):
        raise ValueError('No matched Android factory policy')
    if selected.get('abi')!=ABIS.get(host) or selected['abi'] not in value.get('images',{}):raise ValueError('Factory guest ABI mismatch')
    archives=stable_archives(selected.get('archives',{}))
    image=value['images'][selected['abi']]
    if host=='macos-arm64':macos_policy(archives,image)
    return {**archives,'system-image':image}


def stable_archives(archives):
    if set(archives)!= {'emulator','platform-tools'} or any(p.get('channel')!='stable' for p in archives.values()):
        raise ValueError('Factory requires stable SDK archives')
    return archives


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
    root=Path(root).resolve();pins=policy(root);work=Path(work)
    mac=host_key()=='macos-arm64'
    stock=stock_identity(pins) if mac else None
    if mac:reserve(work,sum(pin['size_bytes'] for pin in pins.values()))
    work.mkdir(parents=True)
    archives=archives_for(pins,work);output=work/'android.payload'
    if mac:
        extraction_reserve(archives,work);archive_image(archives['system-image'],pins['system-image'])
    with tempfile.TemporaryDirectory(prefix='.sdk-',dir=work) as temporary:
        folder=Path(temporary);sdk=folder/'sdk';sdk.mkdir()
        for role in ('emulator','platform-tools'):
            unpack_sdk(archives[role],folder/role,role);(folder/role/role).rename(sdk/role);checked_revision(sdk,role,pins[role])
        if mac:sdk_programs(sdk)
        pending=folder/'android.payload' if mac else output
        result=build_runtime(sdk,archives['system-image'],pending,pins['system-image']['sha256'])
        if mac:
            if stock_identity(policy(root))!=stock:raise ValueError('Mac stock producer or policy changed during collection')
            pending.rename(output);result['stock']=stock
    return output,result


def qualify(root,payload,home,built=None):
    from android_runtime import stage,verify_sdk
    pins=policy(root);mac=host_key()=='macos-arm64'
    if mac:built_receipt(payload,pins,built)
    home=Path(home).resolve();sdk=stage(payload,home);manifest=verify_sdk(sdk);suffix='.exe' if host_key()=='windows-x64' else ''
    if mac:
        sdk_programs(sdk);image_properties((sdk/'system-images/android-35/default/arm64-v8a/source.properties').read_bytes(),pins['system-image'])
        if manifest.get('image_archive_sha256')!=pins['system-image']['sha256']:raise ValueError('Mac factory image archive identity differs')
    profile=home/'qualification';profile.mkdir()
    inherited={'PATH':os.defpath,'LANG':'C'} if mac else os.environ
    environment={**inherited,'ANDROID_HOME':str(sdk),'ANDROID_SDK_ROOT':str(sdk),'ANDROID_USER_HOME':str(profile/'android'),
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
    if args.verify_programs:result['qualification']=qualify(args.root,output,args.work/'installation',result)
    if args.receipt:
        args.receipt.parent.mkdir(parents=True,exist_ok=True);args.receipt.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({'output':str(output),**result}))
