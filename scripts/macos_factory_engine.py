"""Hosted complete Mac assembly with native images or an owned software engine.

This is a maintainer tool. It uses only its new RUNNER_TEMP namespace, never
selects a user's Docker context, and never publishes images or installers.
"""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile

from artifact_io import digest
from host_platform import host_key

ROOT=Path(__file__).resolve().parent.parent
NAME='factory'


def runner(root):
    if host_key()!='macos-arm64' or os.environ.get('GITHUB_ACTIONS')!='true' or os.environ.get('RUNNER_OS')!='macOS':
        raise ValueError('A disposable Apple Silicon GitHub runner is required')
    if Path(os.environ.get('GITHUB_WORKSPACE','')).resolve()!=root:
        raise ValueError('Factory source differs from the runner checkout')
    parent=Path(os.environ.get('RUNNER_TEMP',''))
    if not parent.is_absolute() or parent.is_symlink() or not parent.is_dir():
        raise ValueError('Runner temporary directory is unavailable')
    return parent.resolve()


def qemu(root):
    policy=json.loads((root/'config/macos-factory.json').read_text())
    if policy.get('format')!=1 or re.fullmatch(r'\d+\.\d+\.\d+',str(policy.get('qemu_version'))) is None:
        raise ValueError('Invalid maintainer QEMU policy')
    executable=shutil.which('qemu-system-aarch64')
    if not executable:raise ValueError('The maintainer QEMU tool is missing')
    executable=Path(executable).resolve(strict=True)
    version=subprocess.check_output([executable,'--version'],text=True,timeout=15)
    if re.match(r'QEMU emulator version '+re.escape(policy['qemu_version'])+r'(?:\s|$)',version) is None:
        raise ValueError('Maintainer QEMU differs from its pinned version')
    firmware=executable.parent.parent/'share/qemu/edk2-aarch64-code.fd'
    if not firmware.is_file() or firmware.is_symlink():raise ValueError('The matched QEMU ARM firmware is missing')
    return policy,executable,firmware


def inputs(root,work):
    from container_host_runtime import policy,unpack_lima
    from linux_client_factory import fetch
    selected=policy(root);cache=work/'inputs';cache.mkdir()
    archive=fetch(cache,'lima.tar.gz',selected['lima'])
    lima=work/'lima';unpack_lima(archive,lima)
    image=fetch(cache,'alpine.iso',selected['runtime']['image'])
    packages=cache/'apks';packages.mkdir()
    for pin in selected['runtime']['packages']:fetch(packages,pin['file'],pin)
    return {'command':lima/'bin/limactl','image':image,'packages':packages,
            'arch':'aarch64','vm_type':'qemu','image_sha256':selected['runtime']['image']['sha256']}


def definition(root,work,runtime,policy,firmware):
    from container_host_runtime import definition as host_definition
    value=host_definition(work,runtime,policy['engine_memory'])
    value.update(cpus=policy['engine_cpus'],disk=policy['engine_disk'],cpuType={'aarch64':'max'})
    value['firmware']={'images':[{'location':str(firmware),'arch':'aarch64','digest':'sha256:'+digest(firmware)}]}
    value['mounts']=[{'location':str(path),'writable':True} for path in (root,work)]
    # A fresh factory engine needs the platform-aware image store used by the
    # producer. This changes only this new guest, not the installed VZ policy.
    script=value['provision'][1]['script']
    value['provision'][1]['script']=script.replace('rc-service docker start',
        "mkdir -p /etc/docker\nprintf '%s' '{\"features\":{\"containerd-snapshotter\":true}}' > /etc/docker/daemon.json\nrc-service docker start")
    return value


def wrappers(root,work,runtime,executable):
    folder=work/'bin';folder.mkdir()
    qemu_wrapper=folder/'qemu-system-aarch64'
    adapter=root/'scripts/macos_factory_qemu.py'
    qemu_wrapper.write_text('#!/bin/sh\nexec '+shlex.quote(sys.executable)+' '+shlex.quote(str(adapter))+' "$@"\n')
    qemu_wrapper.chmod(0o755)
    command=folder/'docker'
    command.write_text('#!/bin/sh\nexec '+shlex.quote(str(runtime['command']))+' shell --workdir '+shlex.quote(str(root))+' '+NAME+' sudo docker "$@"\n')
    command.chmod(0o755)
    environment={key:value for key,value in os.environ.items() if key not in ('DOCKER_HOST','DOCKER_CONTEXT','DOCKER_TLS_VERIFY','DOCKER_CERT_PATH')}
    environment.update(LIMA_HOME=str(work/'v'),QEMU_SYSTEM_AARCH64=str(qemu_wrapper),
        AI_PHONE_FACTORY_QEMU=str(executable),PATH=str(folder)+os.pathsep+os.environ.get('PATH',''))
    return environment


def stop_command(child):
    if child.returncode is not None:return
    try:os.killpg(child.pid,signal.SIGTERM)
    except ProcessLookupError:pass
    try:child.wait(timeout=10)
    except subprocess.TimeoutExpired:
        try:os.killpg(child.pid,signal.SIGKILL)
        except ProcessLookupError:pass
        child.wait()


def command(argv,root,environment,log,*,timeout=600):
    arguments=[str(value) for value in argv]
    child=subprocess.Popen(arguments,cwd=root,env=environment,start_new_session=True,
        stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT)
    try:
        code=child.wait(timeout=timeout)
        if code:raise subprocess.CalledProcessError(code,arguments)
    except BaseException as primary:
        try:stop_command(child)
        except BaseException as cleanup:raise BaseExceptionGroup('Factory command and process cleanup failed',[primary,cleanup])
        raise


def stop_owned(work,runtime,environment,log):
    marker=json.loads((work/'owner.json').read_text())
    if marker!={'name':NAME,'state':str(work/'v')} or environment['LIMA_HOME']!=str(work/'v'):
        raise ValueError('Factory engine owner changed; no engine was stopped')
    if not (work/'v'/NAME).exists():return
    command([runtime['command'],'stop','--force',NAME],work,environment,log,timeout=60)
    command([runtime['command'],'delete','--force',NAME],work,environment,log,timeout=60)


def verify_software_engine(work,executable):
    pid=int((work/'v'/NAME/'qemu.pid').read_text())
    if pid<=0:raise ValueError('Factory QEMU process is missing')
    argv=subprocess.check_output(['ps','-p',str(pid),'-o','command='],text=True,timeout=15)
    if str(executable) not in argv or re.search(r'(?:^|[,\s])accel=tcg(?:[,\s]|$)',argv) is None:
        raise ValueError('Factory QEMU did not select software acceleration')


@contextmanager
def engine_session(root,parent,policy,executable,firmware,receipt):
    work=Path(tempfile.mkdtemp(prefix='af-',dir=parent));cleanable=True
    try:
        runtime=inputs(root,work);environment=wrappers(root,work,runtime,executable)
        (work/'owner.json').write_text(json.dumps({'name':NAME,'state':str(work/'v')}))
        configuration=work/'factory.json';configuration.write_text(json.dumps(definition(root,work,runtime,policy,firmware)))
        cleanable=False
        try:
            receipt['phase']='engine-start'
            command([runtime['command'],'start','--tty=false','--name='+NAME,configuration],root,environment,sys.stdout,timeout=900)
            verify_software_engine(work,executable)
            command([work/'bin/docker','info'],root,environment,sys.stdout)
            receipt.update(phase='complete-client',engine_ready=True,engine_acceleration='tcg')
            yield environment
        except BaseException as primary:
            try:stop_owned(work,runtime,environment,sys.stdout)
            except BaseException as cleanup:raise BaseExceptionGroup('Factory and engine cleanup failed',[primary,cleanup])
            cleanable=True
            raise
        else:
            stop_owned(work,runtime,environment,sys.stdout);cleanable=True
    finally:
        receipt['engine_cleanup_verified']=cleanable
        if cleanable:shutil.rmtree(work)


def complete_client(root,compiler_sdk,environment,receipt,prebuilt_images):
    argv=[sys.executable,root/'scripts/macos_client_factory.py','--compiler-sdk',Path(compiler_sdk).resolve()]
    if prebuilt_images is not None:
        folder,identity=prebuilt_images
        argv+=['--image-candidate',folder,'--image-metadata-sha256',identity]
    command(argv,root,environment,sys.stdout,timeout=6500)
    report=json.loads((root/'dist/macos-full-factory-report.json').read_text())
    if report['artifact']['delivery']['complete_device_bundle'] is not True:
        raise ValueError('The full Mac factory did not produce a complete client')
    receipt.update(phase='complete',complete_device_bundle=True,artifact=report['artifact'])


def build(root,compiler_sdk,*,prebuilt_images=None):
    root=Path(root).resolve();parent=runner(root)
    from macos_client_factory import compiler_environment
    compiler_environment(compiler_sdk)
    from android_factory_stock import reserve
    reserve(root,0)
    receipt={'format':1,'host':'macos-arm64','phase':'engine-inputs','engine_ready':False,
             'complete_device_bundle':False,'binary_publication':False,'physical_installation':'not-tested'}
    target=root/'dist/macos-factory-engine-report.json';target.parent.mkdir(exist_ok=True)
    try:
        if prebuilt_images is not None:
            receipt.update(phase='complete-client',engine_cleanup_verified=True,engine_acceleration='not-required')
            complete_client(root,compiler_sdk,dict(os.environ),receipt,prebuilt_images)
        else:
            policy,executable,firmware=qemu(root)
            with engine_session(root,parent,policy,executable,firmware,receipt) as environment:
                complete_client(root,compiler_sdk,environment,receipt,None)
        return receipt
    finally:
        if sys.exception() is not None:receipt['error_code']='factory_failed'
        target.write_text(json.dumps(receipt,sort_keys=True)+'\n')


if __name__=='__main__':
    from lima_native import cancelled
    signal.signal(signal.SIGTERM,cancelled)
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler-sdk',type=Path,required=True)
    parser.add_argument('--image-candidate',type=Path)
    parser.add_argument('--image-metadata-sha256')
    options=parser.parse_args()
    if (options.image_candidate is None)!=(options.image_metadata_sha256 is None):parser.error('Both image candidate and metadata digest required')
    selected=(options.image_candidate,options.image_metadata_sha256) if options.image_candidate else None
    build(ROOT,options.compiler_sdk,prebuilt_images=selected)
