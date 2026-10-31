"""Maintainer-only offline WSL rootfs factory; no existing container is exported."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tarfile
import tempfile
import uuid
from artifact_io import digest
from client_host_entry import archive_records
from container_host_runtime import package_inputs
from wsl_backend import publish_capsule


def capacity(folder, context_bytes, rootfs_budget):
    # Context + daemon build/export/compression/capsule copies coexist; retain the existing 4 GiB reserve.
    required=4*1024**3+context_bytes+4*rootfs_budget
    if shutil.disk_usage(folder).free<required:
        raise ValueError(f'WSL backend factory requires {required} free bytes (including 4 GiB reserve)')
    return required


def input_budget(source,images,tools,packages):
    paths=[Path(source),Path(images),*Path(tools).rglob('*'),*Path(packages).rglob('*')]
    total=sum(path.stat().st_size for path in paths if path.is_file())
    # The pinned Ubuntu base and unpacked engine each have admission budgets, not optimistic compression estimates.
    rootfs_budget=total+1536*1024**2
    return total,rootfs_budget


def factory_plan(root):
    root=Path(root);value=json.loads((root/'config/wsl-backend.json').read_text())
    if value.get('format')!=1 or value.get('host')!='windows-x64' or value.get('backend_host')!='linux-x64':
        raise ValueError('Unsupported WSL backend policy')
    if re.fullmatch(r'ubuntu@sha256:[0-9a-f]{64}',value.get('base_image','')) is None:
        raise ValueError('WSL base image must be immutable')
    engine=json.loads((root/'config/container-host-runtime.json').read_text())['hosts']['linux-x64']
    return {**value,'engine_image':engine['builder_image'],'engine_packages':engine['packages'],
            'engine_versions':{'docker':engine['docker'],'compose':engine['compose']}}


def source_input(payload):
    with tarfile.open(payload,'r:gz') as source:
        manifest,records,_=archive_records(source)
        expected={'web','native-services','js-services'}
        if set(manifest['components'])!=expected:raise ValueError('WSL source must contain only its three backend components')
        for name,row in records.items():
            with source.extractfile(name) as body:actual=hashlib.file_digest(body,'sha256').hexdigest()
            if actual!=row['sha256']:raise ValueError('WSL source integrity differs')
        for name in ('runtime/manifest.json','runtime/services/manifest.json','runtime/js-services/manifest.json'):
            if json.load(source.extractfile(name)).get('host')!='linux-x64':raise ValueError('WSL requires Linux x64 backend components')
        services=json.load(source.extractfile('runtime/services/manifest.json'))
        if services.get('vision')!='linux-tesseract-bundled':raise ValueError('WSL requires offline OCR')
    return digest(payload)


def engine_script(program):
    if program not in ('docker','dockerd'):raise ValueError('Unknown engine entry')
    # musl is scoped to engine subprocesses; the Ubuntu/glibc backend inherits neither variable.
    return ('#!/bin/sh\nset -eu\nengine=/opt/aiphone/engine\n'
            'export LD_LIBRARY_PATH="$engine/lib:$engine/usr/lib"\n'
            'export XTABLES_LIBDIR="$engine/usr/lib/xtables"\n'
            'PATH="$engine/usr/bin:$engine/usr/sbin:$engine/bin:$engine/sbin:$PATH"\n'
            'export PATH\nexec "$engine/usr/bin/'+program+'" "$@"\n')


def os_release_content(path):
    try:
        with os.fdopen(os.open(path,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK),'rb') as source:
            if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):raise ValueError('Unverified factory OS release file')
            content=source.read(65537)
    except OSError as error:raise ValueError('Unverified factory OS release file') from error
    if not content or len(content)>65536:raise ValueError('Unverified factory OS release file')
    return content


def normalize_os_release(root):
    root=Path(root).resolve(strict=True);release=root/'etc/os-release'
    if any(path.is_symlink() or not path.is_dir() for path in (root/'etc',root/'usr',root/'usr/lib')):
        raise ValueError('Unverified factory OS release parent')
    alias=release.is_symlink()
    if alias and os.readlink(release) not in ('../usr/lib/os-release','/usr/lib/os-release'):
        raise ValueError('Unexpected factory OS release link')
    content=os_release_content(root/'usr/lib/os-release')
    if not alias:
        if os_release_content(release)!=content:raise ValueError('Factory OS release content differs')
        return
    # This pinned Ubuntu alias is materialized only in the owned factory image.
    # Archive admission continues to require a regular file and its content hash.
    with tempfile.NamedTemporaryFile(dir=release.parent,prefix='.aiphone-os-release-',delete=False) as output:
        temporary=Path(output.name)
        try:
            output.write(content);output.flush();os.fchmod(output.fileno(),0o644);os.fsync(output.fileno())
            os.replace(temporary,release)
        finally:temporary.unlink(missing_ok=True)


def dockerfile(plan):
    return f'''FROM {plan['engine_image']} AS engine
COPY packages/ /packages/
RUN apk --no-network add /packages/*.apk && rm -rf /packages /var/cache/apk/*
FROM {plan['base_image']}
COPY --from=engine / /opt/aiphone/engine/
COPY tools/ /opt/aiphone/tools/
COPY scripts/ /opt/aiphone/bootstrap/
COPY source.payload images.payload /opt/aiphone/bundle/
COPY docker dockerd /usr/local/bin/
COPY wsl.conf /etc/wsl.conf
COPY engine-sources.json /usr/share/doc/aiphone-engine/source-records.json
RUN /opt/aiphone/tools/python/bin/python3 -I -B -c 'import sys;sys.path.insert(0,"/opt/aiphone/bootstrap");from wsl_backend_factory import normalize_os_release;normalize_os_release("/")'
RUN chmod 755 /usr/local/bin/docker /usr/local/bin/dockerd && \\
    ln -s /opt/aiphone/engine/lib/ld-musl-x86_64.so.1 /lib/ld-musl-x86_64.so.1 && \\
    mkdir -p /usr/local/lib/docker/cli-plugins && \\
    ln -s /opt/aiphone/engine/usr/libexec/docker/cli-plugins/docker-compose /usr/local/lib/docker/cli-plugins/docker-compose
RUN /opt/aiphone/tools/python/bin/python3 -I -B -c 'import sys;sys.path.insert(0,"/opt/aiphone/bootstrap");from wsl_engine_inputs import relocate_links;relocate_links("/opt/aiphone/engine")'
RUN /usr/local/bin/docker --version && /usr/local/bin/docker compose version && /usr/local/bin/dockerd --version
RUN /opt/aiphone/tools/python/bin/python3 -I -B /opt/aiphone/bootstrap/wsl_backend_entry.py --verify
ENTRYPOINT ["/opt/aiphone/tools/python/bin/python3", "-I", "-B", "/opt/aiphone/bootstrap/wsl_backend_entry.py"]
'''


def validated_inputs(root,source,images,tools,packages):
    from installer_tools import verify as verify_tools
    from container_images import stage
    plan=factory_plan(root);source_hash=source_input(source)
    tool_manifest=verify_tools(tools,'linux-x64');image_manifest=stage(images).manifest
    if image_manifest['architecture']!='amd64':raise ValueError('WSL requires amd64 container images')
    package_inputs(packages,plan['engine_packages'])
    return plan,source_hash,tool_manifest


def payload_scripts(source,scripts):
    scripts.mkdir()
    with tarfile.open(source,'r:gz') as archive:
        for entry in archive:
            if entry.isfile() and entry.name.startswith('scripts/') and '/' not in entry.name[8:]:
                with archive.extractfile(entry) as body:(scripts/entry.name[8:]).write_bytes(body.read())
    if not (scripts/'wsl_backend_entry.py').is_file():raise ValueError('Rebuild backend source with its WSL entry')


def prepare_context(root,folder,source,images,tools,packages):
    plan,source_hash,tool_manifest=validated_inputs(root,source,images,tools,packages)
    folder.mkdir()
    for path,name in ((source,'source.payload'),(images,'images.payload')):shutil.copyfile(path,folder/name)
    shutil.copytree(tools,folder/'tools');shutil.copytree(packages,folder/'packages')
    payload_scripts(source,folder/'scripts')
    for name in ('docker','dockerd'):(folder/name).write_text(engine_script(name))
    (folder/'wsl.conf').write_text('[interop]\nenabled=false\nappendWindowsPath=false\n[automount]\nenabled=false\n')
    from wsl_engine_inputs import licenses
    (folder/'engine-sources.json').write_text(json.dumps(licenses(packages,plan['engine_packages']),sort_keys=True)+'\n')
    (folder/'Dockerfile').write_text(dockerfile(plan))
    return {'policy':plan,'source_sha256':source_hash,'images_sha256':digest(images),
            'tools_manifest_sha256':digest(Path(tools)/'manifest.json'),'tools':tool_manifest['tools']}


def run(argv,**kwargs):
    return subprocess.run(list(map(str,argv)),check=True,**kwargs)


def export_owned(image,output,name):
    marker='wsl-backend-factory'
    created=run(['docker','create','--name',name,'--network','none','--label','dev.aiphone.factory='+marker,image],capture_output=True,text=True)
    identity=created.stdout.strip();owned=False
    if re.fullmatch('[0-9a-f]{64}',identity) is None:raise ValueError('Invalid factory container identity')
    try:
        record=json.loads(subprocess.check_output(['docker','inspect',identity],text=True))[0]
        if record['Config']['Labels'].get('dev.aiphone.factory')!=marker or record['Config']['Image']!=image:
            raise ValueError('Factory container ownership differs')
        owned=True
        with output.open('xb') as body:run(['docker','export',identity],stdout=body)
    finally:
        if owned:run(['docker','rm',identity],stdout=subprocess.DEVNULL)


def build(root,work,source,images,tools,packages):
    root,work=Path(root),Path(work)
    capacity(work.parent,*input_budget(source,images,tools,packages))
    if work.exists():raise FileExistsError('Use a new WSL factory directory; existing inputs preserved')
    work.mkdir();plan=prepare_context(root,work/'context',source,images,tools,packages)
    nonce=uuid.uuid4().hex;image='ai-phone-wsl-factory:'+nonce;name='ai-phone-wsl-factory-'+nonce
    run(['docker','buildx','build','--platform','linux/amd64','--network','none','--load','--provenance=false','-t',image,work/'context'])
    archive=work/'rootfs.tar';export_owned(image,archive,name)
    import gzip
    compressed=work/'rootfs.tar.gz'
    with archive.open('rb') as source_file,compressed.open('xb') as raw,gzip.GzipFile(fileobj=raw,mode='wb',mtime=0,filename='') as output:
        shutil.copyfileobj(source_file,output,1024**2)
    result=publish_capsule(compressed,work/'wsl-backend.payload',plan)
    (work/'receipt.json').write_text(json.dumps(result,indent=2)+'\n')
    return result


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parent.parent)
    for name in ('work','source','images','tools','packages'):parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args();print(json.dumps(build(args.root,args.work,args.source,args.images,args.tools,args.packages)))
