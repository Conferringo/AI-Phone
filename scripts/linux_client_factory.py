"""Assemble and compile the complete Linux x64 client from reviewed public inputs.

This is a maintainer factory, not an end-user bootstrap. It never exports a
container or reads user state. Binary publication remains a separate operation.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import uuid
from artifact_io import digest
from client_host import download
from host_platform import host_key


def run(argv,root=None,**kwargs):
    return subprocess.run([str(arg) for arg in argv],cwd=root,check=True,**kwargs)


def fetch(folder,name,pin):
    if Path(name).name!=name:raise ValueError('Invalid factory download name')
    target=download(pin,Path(folder)/name)
    if 'size_bytes' in pin and target.stat().st_size!=pin['size_bytes']:raise ValueError('Factory download size differs')
    return target


def checked_delivery(components):
    from client_delivery import delivery_metadata
    value=delivery_metadata(components,'linux-x64')
    if not value['complete_device_bundle']:raise ValueError('Incomplete Linux factory resources')
    return value


def image_namespace(source,nonce):
    if re.fullmatch('[0-9a-f]{64}',source) is None or re.fullmatch('[0-9a-f]{12}',nonce) is None:
        raise ValueError('Invalid factory identity')
    return 'ai-phone-factory/'+source[:12]+'-'+nonce


def docker_image(reference):
    value=subprocess.run(['docker','image','inspect',reference],capture_output=True,text=True)
    return json.loads(value.stdout)[0] if value.returncode==0 else None


def check_docker():
    version=json.loads(subprocess.check_output(['docker','version','--format','{{json .}}'],text=True))
    for role in ('Client','Server'):
        revision=str(version[role]['ApiVersion'])
        if re.fullmatch(r'\d+\.\d+',revision) is None or tuple(map(int,revision.split('.')))<(1,49):
            raise ValueError('Docker factory requires client and engine API 1.49 or newer')
    info=json.loads(subprocess.check_output(['docker','info','--format','{{json .}}'],text=True))
    if dict(info.get('DriverStatus') or []).get('driver-type')!='io.containerd.snapshotter.v1':
        raise ValueError('Docker factory requires the platform-aware containerd image store')


def build_image(root,role,reference,*,workspace=None,clash=None):
    if role not in ('workspace','desktop'):raise ValueError('Unknown factory image role')
    if docker_image(reference) is not None:raise ValueError('Factory image reference already exists')
    command=['docker','buildx','build','--platform','linux/amd64','--load','--provenance=false','--tag',reference]
    if role=='desktop':
        if not workspace or clash is None:raise ValueError('Desktop factory needs its workspace and Clash application')
        command+=['--build-arg','WORKSPACE_IMAGE='+workspace,'--build-context','clash_apps='+str(Path(clash).resolve())]
    run([*command,Path(root)/'infra'/role],root)
    return reference


def prepare_android(root,cache):
    from android_component_factory import build
    return build(root,cache/'android')


def factory_context(root,folder,role,python_archive=None):
    folder.mkdir(parents=True)
    for source,target in [(root/'scripts'/role/'Dockerfile','Dockerfile'),(root/'scripts'/role/'collect.py','collect.py'),(root/'scripts/linux_dependency_factory.py','factory.py')]:
        shutil.copyfile(source,folder/target)
    if python_archive is not None:shutil.copyfile(python_archive,folder/'python.tar.gz')
    output=folder/'output'
    run(['docker','buildx','build','--platform','linux/amd64','--target','export','--output','type=local,dest='+str(output),folder],root)
    return output


def prepare_host(root,cache,python_archive):
    from linux_host_runtime import build as build_native
    from linux_ocr_runtime import build as build_ocr
    from container_host_runtime import policy,build
    for role,name,producer,destination in [('linux-host','linux-x64-host.tar.gz',build_native,root/'.tools/linux-host-runtime'),
                                          ('linux-ocr','linux-x64-ocr.tar.gz',build_ocr,root/'.tools/linux-ocr-runtime')]:
        output=factory_context(root,cache/'contexts'/role,role,python_archive if role=='linux-ocr' else None)
        producer(root,output/name,destination)
    selected=policy(root);image=fetch(cache,'alpine.iso',selected['runtime']['image']);lima=fetch(cache,'lima.tar.gz',selected['lima'])
    packages=cache/'apks';packages.mkdir()
    for pin in selected['runtime']['packages']:fetch(packages,pin['file'],pin)
    result=build(root,lima,image,packages)
    return root/'.tools/linux-ocr-runtime',result


def prepare_tools(root,cache):
    from installer_tools import build
    pins=json.loads((root/'config/installer-tools.json').read_text())['hosts']['linux-x64']
    archives=cache/'tools-inputs'
    for role,pin in pins.items():fetch(archives,pin['file'],pin)
    output=cache/'tools';result=build(archives,output,pins=pins)
    return output,archives/'python.tar.gz',result


def prepare_apps(root,env):
    from prebuilt_phone import build as phone_build
    from prebuilt_guest_apps import build as guest_build,pins
    phone=phone_build(root,env)
    for pin in pins(root).values():fetch(root/'.tools/clash-apps/android',pin['file'],pin)
    guest=guest_build(root,Path(env['ANDROID_HOME'])/'build-tools/35.0.0',env)
    return {'phone':phone,'guest':guest}


def database_images(root):
    text=(root/'compose.yaml').read_text();result={}
    for role in ('postgres','redis'):
        matches=re.findall(r'\$\{AI_PHONE_'+role.upper()+r'_IMAGE:-([^}]+)\}',text)
        if len(matches)!=1 or re.fullmatch(role+r':[^@]+@sha256:[0-9a-f]{64}',matches[0]) is None:
            raise ValueError('Missing immutable database image pin')
        reference=role+'@'+matches[0].split('@',1)[1];run(['docker','pull','--platform','linux/amd64',reference],root);result[role]=reference
    return result


def prepare_images(root,cache,source):
    from build_container_images import build
    pins=json.loads((root/'config/clash-apps.json').read_text())['alternatives']['linux-amd64']
    clash=cache/'clash';fetch(clash,pins['file'],pins)
    namespace=image_namespace(source,uuid.uuid4().hex[:12]);sources=database_images(root)
    sources['workspace']=build_image(root,'workspace',namespace+'/workspace:build')
    sources['desktop']=build_image(root,'desktop',namespace+'/desktop:build',workspace=sources['workspace'],clash=clash)
    output=cache/'images.payload';return output,build(sources,output)


def run_checks(root,**options):
    return run([Path(root)/'scripts/bun','run','check'],root,**options)


def compile_client(root,android,images,tools,ocr):
    run_checks(root)
    run([os.sys.executable,root/'scripts/build_client.py','--platform','linux','--with-web-runtime','--with-service-runtime','--with-phone-apps',
         '--with-container-host','--with-js-services','--with-guest-apps','--android-runtime',android,'--images-runtime',images,'--installer-tools',tools,'--linux-ocr',ocr],root)
    from prepare_client_host import payload_components
    checked_delivery(payload_components(root))
    version=json.loads((root/'config/release-policy.json').read_text())['version'];artifact=root/'dist'/('AI-Phone-'+version+'-linux-amd64.deb')
    receipt=json.loads(artifact.with_suffix('.deb.manifest.json').read_text())
    if not receipt['complete_device_bundle'] or receipt['sha256']!=digest(artifact):raise ValueError('Compiled Linux client is incomplete or changed')
    return {'file':artifact.name,'size':artifact.stat().st_size,'sha256':receipt['sha256'],'delivery':checked_delivery(receipt['prebuilt_components'])}


def build(root):
    root=Path(root).resolve()
    if host_key()!='linux-x64':raise ValueError('Complete Linux factory requires a Linux x64 host')
    if shutil.disk_usage(root).free<35*1024**3:raise ValueError('Complete Linux factory requires 35 GiB free workspace')
    check_docker()
    from check_source import check
    check(root);cache=root/'.tools/complete-linux'
    if cache.exists():raise ValueError('Use a fresh factory checkout; existing outputs were preserved')
    cache.mkdir(parents=True);source=digest(root/'SOURCE-MANIFEST.json')
    tools,python_archive,tool_result=prepare_tools(root,cache)
    ocr,host=prepare_host(root,cache,python_archive)
    android,phone_runtime=prepare_android(root,cache)
    apps=prepare_apps(root,dict(os.environ))
    images,image_result=prepare_images(root,cache,source)
    artifact=compile_client(root,android,images,tools,ocr)
    result={'format':1,'host':'linux-x64','source_commit':os.environ.get('GITHUB_SHA',''),'source_manifest_sha256':source,
            'artifact':artifact,'tools':tool_result,'container_host':host,'android':phone_runtime,
            'image_capsule_bytes':image_result['bytes'],'phone_apps_source_sha256':apps['phone']['source_sha256'],
            'physical_installation':'not-tested-maintainer-waiver','binary_publication':False}
    output=root/'dist/linux-full-factory-report.json';output.write_text(json.dumps(result,indent=2)+'\n');return result


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parent.parent);args=parser.parse_args()
    print(json.dumps(build(args.root)))
