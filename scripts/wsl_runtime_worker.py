"""PID 1 worker inside a newly owned, offline privileged qualification container."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys

STATE=Path('/var/lib/aiphone')


def next_phase(marker,nonce):
    if not marker.exists():return 1
    value=json.loads(marker.read_text())
    if value!={'nonce':nonce,'phase':1}:raise ValueError('Qualification marker owner or phase differs')
    return 2


def fixture_identity():
    nonce=os.environ.get('AI_PHONE_WSL_QUALIFICATION_NONCE','')
    if re.fullmatch('[0-9a-f]{32}',nonce) is None or os.environ.get('container')!='docker' or not Path('/.dockerenv').is_file() or os.getpid()!=1:
        raise ValueError('Only the owned disposable container PID 1 can run this worker')
    return nonce


def nested_cgroups():
    # This is the fixture's private cgroup namespace, never a host bind mount.
    root=Path('/sys/fs/cgroup')
    controllers=root/'cgroup.controllers'
    if controllers.exists():
        group=root/'aiphone-qualification';group.mkdir(exist_ok=True)
        for pid in (root/'cgroup.procs').read_text().splitlines():
            try:(group/'cgroup.procs').write_text(pid)
            except ProcessLookupError:pass
        (root/'cgroup.subtree_control').write_text(' '.join('+'+name for name in controllers.read_text().split()))
    subprocess.run(['/bin/mount','--make-rshared','/'],check=True,timeout=15)


def checkpoint(receipt,stage):
    receipt['stage']=stage
    from database import atomic_json
    atomic_json(STATE/'qualification-result.json',receipt)


def initialize():
    import wsl_backend_entry as entry
    from database import atomic_json
    request=STATE/'qualification-request.json'
    atomic_json(request,{'locale':'en','phone':{'cpus':2,'memory_mb':2048,'storage_gb':16},'desktop':{'cpus':2,'memory_mb':2048}})
    result=entry.initialize(request)
    if result['phone_ready'] or result['windows_boot_verified']:raise ValueError('Backend initialization overclaimed readiness')
    return entry.staged(entry.INSTALLATION)


def shutdown(root):
    if root is None:return
    from linux_services import stop_unit
    from wsl_runtime_probes import execute
    # Every Docker command inherits the WSL-owned socket selected by initialize.
    for name in ('channels','media','web','assistant'):stop_unit(root,name)
    execute(root,['docker','compose','-f','compose.desktop.yaml','stop','--timeout','15','desktop'])
    execute(root,['docker','compose','-f','compose.yaml','-f','compose.dev.yaml','stop','--timeout','15','postgres','redis'])
    stop_unit(root,'wsl-engine')


def perform(receipt,nonce,phase):
    import wsl_runtime_probes as probe
    checkpoint(receipt,'nesting');nested_cgroups()
    checkpoint(receipt,'initialize');root=None;completed=False
    try:
        root=initialize()
        checkpoint(receipt,'readiness');receipt['checks'].update(probe.ready(root));receipt['checks'].update(probe.database_roles(root))
        checkpoint(receipt,'desktop')
        device=probe.desktop_registration(root,nonce,phase)
        receipt['checks']['desktop_registered']=True
        receipt['frame']=probe.desktop_frame(root);receipt['checks']['desktop_frame']=True
        receipt['checks']['desktop_media']=probe.desktop_media(root,device)
        checkpoint(receipt,'persistence')
        receipt['checks']['work_file']=probe.work_file(root,nonce,phase)
        receipt['checks']['database_marker']=probe.database_marker(root,nonce,phase)
        completed=True
    finally:
        if completed:checkpoint(receipt,'stopping')
        shutdown(root)
    checkpoint(receipt,'stopped')


def main():
    from wsl_runtime_fixture import CHECKS
    nonce=fixture_identity();STATE.mkdir(mode=0o700,exist_ok=True)
    marker=STATE/'qualification-marker.json';phase=next_phase(marker,nonce)
    receipt={'format':1,'nonce':nonce,'phase':phase,'status':'failed','stage':'guard','checks':{name:False for name in sorted(CHECKS)},'frame':{},'windows_boot_verified':False,'phone_ready':False}
    checkpoint(receipt,'guard')
    try:
        perform(receipt,nonce,phase)
        if not all(receipt['checks'].values()):raise ValueError('Runtime probe did not complete')
        from database import atomic_json
        atomic_json(marker,{'nonce':nonce,'phase':phase})
        receipt['status']='passed';checkpoint(receipt,'stopped')
    except Exception:
        # Private service logs remain inside the fixture. The public receipt has
        # fixed stage/check fields and never serializes exceptions, tokens or env.
        checkpoint(receipt,receipt['stage'])
        return 1
    return 0


if __name__=='__main__':
    sys.path.insert(0,str(Path(__file__).resolve().parent))
    raise SystemExit(main())
