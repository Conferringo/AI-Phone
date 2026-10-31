"""Explicit GitHub-hosted, privately owned nested-engine runtime qualification."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import uuid
from artifact_io import digest

LABEL='dev.aiphone.qualification'
CHECKS=frozenset(('assistant','web','channels','database_roles','redis','desktop_frame','work_file','database_marker','desktop_registered','desktop_media'))


def require_disposable():
    expected={'GITHUB_ACTIONS':'true','RUNNER_ENVIRONMENT':'github-hosted','RUNNER_OS':'Linux','AI_PHONE_DISPOSABLE_QUALIFICATION':'1'}
    if any(os.environ.get(key)!=value for key,value in expected.items()):
        raise ValueError('Explicit disposable GitHub-hosted Linux qualification is required')


def identity(nonce):
    if re.fullmatch('[0-9a-f]{32}',nonce) is None:raise ValueError('Invalid qualification identity')
    return 'ai-phone-wsl-q-'+nonce


def docker(args,**kwargs):
    return subprocess.run(['docker',*map(str,args)],check=True,capture_output=True,text=True,timeout=kwargs.pop('timeout',180),**kwargs).stdout.strip()


def runtime_capacity(folder,rootfs_bytes,source_bytes,image_bytes):
    required=4*1024**3+2*rootfs_bytes+source_bytes+2*image_bytes+1024**3
    if shutil.disk_usage(folder).free<required:raise ValueError(f'Qualification needs {required} free bytes including 4 GiB reserve')
    return required


def checked_inputs(work,source,images):
    from wsl_backend import verify_capsule
    from wsl_backend_factory import source_input
    from container_images import stage
    from client_host_entry import archive_records
    import tarfile
    manifest=verify_capsule(work/'wsl-backend.payload')
    if digest(work/'rootfs.tar.gz')!=manifest['rootfs']['sha256']:raise ValueError('Rootfs changed before runtime import')
    source_hash=source_input(source);prepared=stage(images)
    if source_hash!=manifest['provenance']['source_sha256'] or prepared.identity!=manifest['provenance']['images_sha256']:
        raise ValueError('Runtime qualification inputs differ from assembled rootfs')
    with tarfile.open(source,'r:gz') as archive:_,records,_=archive_records(archive)
    budget=runtime_capacity(work,manifest['inventory']['uncompressed_bytes'],sum(row['size'] for row in records.values()),sum(row['size'] for row in prepared.manifest['images'].values()))
    return {'rootfs_sha256':manifest['rootfs']['sha256'],'source_sha256':source_hash,'images_sha256':prepared.identity,'required_free_bytes':budget}


def create_command(nonce,image):
    name=identity(nonce)
    return ['create','--name',name,'--label',LABEL+'='+nonce,'--privileged','--network=none','--cgroupns=private',
            '--mount','type=volume,source='+name+',target=/var/lib/aiphone','--tmpfs','/run:rw,mode=755','--tmpfs','/tmp:rw,mode=1777',
            '--env','AI_PHONE_WSL_QUALIFICATION_NONCE='+nonce,'--env','container=docker',image,
            '/opt/aiphone/tools/python/bin/python3','-I','-B','/opt/aiphone/bootstrap/wsl_runtime_worker.py']


def owned(kind,reference,nonce,run=docker):
    row=json.loads(run([kind,'inspect',reference]))[0]
    labels=row.get('Labels',{}) if kind=='volume' else row.get('Config',{}).get('Labels',{})
    actual=row.get('Name') if kind=='volume' else row.get('Id')
    if actual!=reference or labels.get(LABEL)!=nonce:raise ValueError('Qualification resource ownership changed')
    return row


def cleanup(nonce,resources,run=docker):
    result={}
    for kind in ('container','image','volume'):
        if kind not in resources:continue
        try:
            owned(kind,resources[kind],nonce,run)
            run([kind,'rm',*(['--force'] if kind=='container' else []),resources[kind]])
            result[kind]=True
        except (ValueError,OSError,subprocess.SubprocessError):result[kind]=False
    return result


def check_worker_identity(value,nonce,phase):
    expected={'format','nonce','phase','status','stage','checks','frame','windows_boot_verified','phone_ready','initialization'}
    if not isinstance(value,dict) or set(value)!=expected:raise ValueError('Invalid runtime evidence fields')
    identity_fields={'format':1,'nonce':nonce,'phase':phase,'windows_boot_verified':False,'phone_ready':False}
    if any(type(value[key]) is not type(expected) or value[key]!=expected for key,expected in identity_fields.items()):
        raise ValueError('Invalid runtime evidence identity or readiness')


def check_worker_frame(frame):
    if not isinstance(frame,dict):raise ValueError('Invalid frame evidence type')
    if not frame:return
    if not isinstance(frame,dict) or set(frame)!= {'width','height','sha256'}:raise ValueError('Invalid frame evidence fields')
    if any(type(frame[k]) is not int or not 1<=frame[k]<=8192 for k in ('width','height')):raise ValueError('Invalid frame dimensions')
    if not isinstance(frame['sha256'],str) or re.fullmatch('[0-9a-f]{64}',frame['sha256']) is None:raise ValueError('Invalid frame digest')


def check_worker_checks(checks):
    if not isinstance(checks,dict) or set(checks)!=CHECKS:raise ValueError('Invalid runtime check fields')
    if any(type(item) is not bool for item in checks.values()):raise ValueError('Invalid runtime checks')


def worker_result(value,nonce,phase):
    from wsl_initialize_diagnostics import checked,initial
    check_worker_identity(value,nonce,phase)
    if value['status'] not in ('passed','failed') or value['stage'] not in ('guard','nesting','initialize','readiness','desktop','persistence','stopping','stopped'):
        raise ValueError('Invalid runtime evidence stage')
    check_worker_checks(value['checks']);check_worker_frame(value['frame'])
    checked(value['initialization'],passed=value['status']=='passed')
    if value['stage'] in ('guard','nesting') and value['initialization']!=initial():raise ValueError('Premature initialization evidence')
    if value['stage'] in ('readiness','desktop','persistence','stopping','stopped'):checked(value['initialization'],passed=True)
    if value['status']=='passed' and (not all(value['checks'].values()) or not value['frame'] or value['stage']!='stopped'):raise ValueError('Incomplete runtime evidence')
    return value


def read_phase(work,container,nonce,phase,run=docker):
    owned('container',container,nonce,run)
    destination=work/f'worker-phase-{phase}.json'
    run(['cp',container+':/var/lib/aiphone/qualification-result.json',destination])
    if destination.stat().st_size>65536:raise ValueError('Runtime evidence exceeds budget')
    return worker_result(json.loads(destination.read_text()),nonce,phase)


def run_phase(work,container,nonce,phase,run=docker):
    owned('container',container,nonce,run)
    failed=False
    with (work/f'worker-phase-{phase}.private.log').open('xb') as output:
        try:failed=subprocess.run(['docker','start','--attach',container],stdout=output,stderr=subprocess.STDOUT,timeout=900).returncode!=0
        except subprocess.TimeoutExpired:failed=True
    result=read_phase(work,container,nonce,phase,run)
    if failed:result['status']='failed'
    return result


def materialize(work,nonce,resources,run=docker):
    name=identity(nonce)
    image=run(['image','import','--change','LABEL '+LABEL+'='+nonce,work/'rootfs.tar.gz',name+':fixture'],timeout=600)
    if re.fullmatch('sha256:[0-9a-f]{64}',image) is None:raise ValueError('Invalid imported fixture identity')
    resources['image']=image;owned('image',image,nonce,run)
    volume=run(['volume','create','--label',LABEL+'='+nonce,name])
    if volume!=name:raise ValueError('Invalid fixture volume identity')
    resources['volume']=volume;owned('volume',volume,nonce,run)
    container=run(create_command(nonce,image))
    if re.fullmatch('[0-9a-f]{64}',container) is None:raise ValueError('Invalid fixture container identity')
    resources['container']=container;owned('container',container,nonce,run)
    return container


def qualify_runtime(work,source,images):
    require_disposable();work=Path(work)
    inputs=checked_inputs(work,source,images);nonce=uuid.uuid4().hex;resources={}
    receipt={'format':1,'inputs':inputs,'phases':[],'cleanup':{},'stage':'materialize','passed':False,'windows_boot_verified':False,'phone_ready':False}
    try:
        container=materialize(work,nonce,resources)
        for phase in (1,2):
            receipt['stage']='phase_'+str(phase)
            result=run_phase(work,container,nonce,phase);receipt['phases'].append(result)
            if result['status']!='passed':raise ValueError('Qualification worker did not pass')
        receipt['passed']=True;receipt['stage']='complete'
        return receipt
    except Exception as error:
        receipt['failure_type']=type(error).__name__
        raise
    finally:
        receipt['cleanup']=cleanup(nonce,resources)
        if not all(receipt['cleanup'].values()):receipt['passed']=False;receipt['stage']='cleanup'
        (work/'runtime-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
        if not all(receipt['cleanup'].values()):raise ValueError('Owned qualification cleanup was incomplete')
