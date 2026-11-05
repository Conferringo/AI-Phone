"""Clean Linux runner qualification for the WSL backend; does not qualify Windows boot."""
import json
import os
from pathlib import Path
import shutil
from artifact_io import digest
from host_platform import host_key
from linux_client_factory import check_docker,prepare_tools,factory_context,prepare_images,fetch,run
from wsl_backend_factory import factory_plan,build


def qualification_preflight(root):
    if host_key()!='linux-x64':raise ValueError('WSL backend qualification needs a disposable Linux x64 runner')
    if shutil.disk_usage(root).free<4*1024**3:raise ValueError('Keep 4 GiB free before preparing backend inputs')
    check_docker()
    from check_source import check
    check(root)
    work=root/'.tools/wsl-backend-qualification'
    if work.exists():raise FileExistsError('Use a fresh owned qualification workspace')
    return work


def assemble(root,*,device_apps=False):
    root=Path(root).resolve();work=qualification_preflight(root)
    work.mkdir(parents=True)
    if device_apps:
        from linux_client_factory import prepare_apps
        prepare_apps(root,dict(os.environ))
    tools,python_archive,_=prepare_tools(root,work)
    from wsl_phone_tools import prepare as prepare_phone_tools
    phone_tools=prepare_phone_tools(root,work/'phone-tools')
    output=factory_context(root,work/'ocr-context','linux-ocr',python_archive)
    from linux_ocr_runtime import build as build_ocr
    ocr=root/'.tools/linux-ocr-runtime';build_ocr(root,output/'linux-x64-ocr.tar.gz',ocr)
    from build_native_services import build as build_services
    from prebuilt_js import build as build_js
    build_services(root)
    run([root/'scripts/bun','run','build'],root)
    build_js(root)
    from prepare_client_host import prepare
    prepare(root,with_web_runtime=True,with_service_runtime=True,with_js_services=True,
            with_phone_apps=device_apps,with_guest_apps=device_apps,linux_ocr=ocr)
    from factory_cache import retire_native_cache,retire_image_build_cache
    retired=retire_native_cache(root,root/'apps/client/src-tauri/gen/host/source.payload')
    (work/'cache-retirement.json').write_text(json.dumps(retired,sort_keys=True)+'\n')
    images,_=prepare_images(root,work,digest(root/'SOURCE-MANIFEST.json'))
    retired['image_build_cache']=retire_image_build_cache(root,images)
    (work/'cache-retirement.json').write_text(json.dumps(retired,sort_keys=True)+'\n')
    packages=work/'packages';packages.mkdir()
    for row in factory_plan(root)['engine_packages']:fetch(packages,row['file'],row)
    result=build(root,work/'assembled',root/'apps/client/src-tauri/gen/host/source.payload',images,tools,packages,phone_tools)
    return result,work,images


def qualify(root,*,device_apps=False):
    from wsl_runtime_fixture import require_disposable,qualify_runtime
    require_disposable();root=Path(root).resolve()
    report={'format':1,'stage':'assemble','passed':False,'source_commit':os.environ.get('GITHUB_SHA',''),
            'windows_boot_verified':False,'phone_ready':False,'complete_device_bundle':False}
    (root/'dist').mkdir(exist_ok=True)
    try:
        component,work,images=assemble(root,device_apps=device_apps);report['component']=component;report['stage']='runtime'
        report['runtime']=qualify_runtime(work/'assembled',root/'apps/client/src-tauri/gen/host/source.payload',images)
        report['passed']=True;report['stage']='complete'
        return report
    finally:
        retired=root/'.tools/wsl-backend-qualification/cache-retirement.json'
        if retired.is_file():report['cache_retirement']=json.loads(retired.read_text())
        runtime=root/'.tools/wsl-backend-qualification/assembled/runtime-receipt.json'
        if runtime.is_file():report['runtime']=json.loads(runtime.read_text())
        (root/'dist/wsl-backend-qualification.json').write_text(json.dumps(report,indent=2)+'\n')


if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--with-device-apps',action='store_true',help='Build and embed phone control and pinned guest apps; requires Android 35 compiler tools and Java 17 on the maintainer host')
    args=parser.parse_args()
    print(json.dumps(qualify(Path(__file__).resolve().parent.parent,device_apps=args.with_device_apps)))
