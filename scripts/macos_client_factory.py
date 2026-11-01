"""Maintainer factory for the complete Apple Silicon client, without user state.

Requires a matched ARM64 Docker build engine and an Android application compiler
SDK/Java 17. These are build-machine tools; the resulting client embeds its own
runtime and does not require them on an end user's Mac.
"""
import argparse
import json
import os
from pathlib import Path
import signal
import shutil
import tarfile
from artifact_io import digest
from host_platform import host_key
from linux_client_factory import fetch,run,check_docker,prepare_tools,prepare_apps,prepare_images,run_checks


def compiler_environment(sdk):
    sdk=Path(sdk).resolve()
    java=Path(os.environ.get('JAVA_HOME',''))
    required=['platforms/android-35/android.jar',*[f'build-tools/35.0.0/{name}' for name in ('aapt2','d8','apksigner')]]
    if not all((sdk/name).is_file() for name in required) or not (java/'bin/javac').is_file():
        raise ValueError('Factory requires the Android 35 application compiler and JAVA_HOME for Java 17')
    return {**os.environ,'ANDROID_HOME':str(sdk),'ANDROID_SDK_ROOT':str(sdk),'JAVA_HOME':str(java.resolve())}


def native_lima(root,cache):
    from lima_native import build_policy,build,cached_build
    if cached_build(root) is not None:return
    pin=build_policy(root);archive=fetch(cache,'go.tar.gz',pin['go_archive'])
    folder=cache/'go-compiler';folder.mkdir()
    from android_factory_stock import reserve
    with tarfile.open(archive,'r:gz') as source:
        reserve(folder,sum(entry.size for entry in source))
        source.extractall(folder,filter='data')
    source=cache/'lima-source';source.mkdir()
    run(['git','init',source],root)
    run(['git','remote','add','origin',pin['source_url']],source)
    run(['git','fetch','--no-tags','--depth=1','origin',pin['source_commit']],source,timeout=120)
    run(['git','checkout','--detach','FETCH_HEAD'],source)
    build(root,source,folder/'go/bin/go')


def prepare_host(root,cache):
    from container_host_runtime import policy,build
    selected=policy(root);native_lima(root,cache)
    image=fetch(cache,'alpine.iso',selected['runtime']['image'])
    lima=fetch(cache,'lima.tar.gz',selected['lima'])
    packages=cache/'apks';packages.mkdir()
    for row in selected['runtime']['packages']:fetch(packages,row['file'],row)
    return build(root,lima,image,packages)


def prepare_android(root,cache):
    from android_component_factory import build,qualify
    payload,receipt=build(root,cache/'android')
    installation=cache/'android-qualification'
    receipt['qualification']=qualify(root,payload,installation,receipt)
    # The freshly generated qualification SDK/profile has no installation data.
    # Assembly consumes only the verified payload, avoiding a retained raw copy.
    shutil.rmtree(installation)
    for role in ('emulator','platform-tools','system-image'):(cache/'android'/(role+'.zip')).unlink()
    return payload,receipt


def checked_delivery(components):
    from client_delivery import delivery_metadata
    value=delivery_metadata(components,'macos-arm64')
    if not value['complete_device_bundle']:raise ValueError('Incomplete macOS factory resources')
    return value


def compile_client(root,android,images,tools,env):
    run_checks(root,env=env)
    run([os.sys.executable,root/'scripts/build_client.py','--platform','macos',
         '--with-web-runtime','--with-service-runtime','--with-phone-apps',
         '--with-container-host','--with-js-services','--with-guest-apps',
         '--android-runtime',android,'--images-runtime',images,'--installer-tools',tools],root,env=env)
    from prepare_client_host import payload_components
    checked_delivery(payload_components(root))
    version=json.loads((root/'config/release-policy.json').read_text())['version']
    artifact=root/'dist'/f'AI-Phone-{version}-macos-arm64.dmg'
    receipt=json.loads(artifact.with_suffix('.dmg.manifest.json').read_text())
    if not receipt['complete_device_bundle'] or receipt['sha256']!=digest(artifact):
        raise ValueError('Compiled macOS client is incomplete or changed')
    return {'file':artifact.name,'size':artifact.stat().st_size,'sha256':receipt['sha256'],
            'delivery':checked_delivery(receipt['prebuilt_components'])}


def build(root,compiler_sdk):
    root=Path(root).resolve()
    if host_key()!='macos-arm64':raise ValueError('Complete macOS factory requires Apple Silicon')
    from android_factory_stock import reserve
    reserve(root,0)
    env=compiler_environment(compiler_sdk)
    check_docker()
    from container_images import daemon_architecture,docker_command
    if daemon_architecture(docker_command)!='arm64':raise ValueError('Mac factory requires an ARM64 Linux build engine')
    from check_source import check
    check(root);cache=root/'.tools/complete-macos'
    if cache.exists():raise ValueError('Use a fresh factory checkout; existing outputs were preserved')
    cache.mkdir(parents=True);source=digest(root/'SOURCE-MANIFEST.json')
    tools,_python,tool_result=prepare_tools(root,cache)
    host=prepare_host(root,cache)
    android,phone_runtime=prepare_android(root,cache)
    apps=prepare_apps(root,env)
    images,image_result=prepare_images(root,cache,source,architecture='arm64')
    artifact=compile_client(root,android,images,tools,env)
    if digest(root/'SOURCE-MANIFEST.json')!=source:raise ValueError('Factory source identity changed during assembly')
    result={'format':1,'host':'macos-arm64','source_commit':os.environ.get('GITHUB_SHA',''),
            'source_manifest_sha256':source,'artifact':artifact,'tools':tool_result,
            'container_host':host,'android':phone_runtime,'image_capsule_bytes':image_result['bytes'],
            'phone_apps_source_sha256':apps['phone']['source_sha256'],
            'physical_installation':'not-tested','binary_publication':False}
    (root/'dist/macos-full-factory-report.json').write_text(json.dumps(result,indent=2)+'\n')
    return result


if __name__=='__main__':
    from lima_native import cancelled
    signal.signal(signal.SIGTERM,cancelled)
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--compiler-sdk',type=Path,required=True)
    args=parser.parse_args()
    print(json.dumps(build(args.root,args.compiler_sdk)))
