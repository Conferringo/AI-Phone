"""Stop only verified project-owned launchd services; retain disks, DB and accounts."""
import os
from pathlib import Path
import subprocess
from service_start import owned_jobs,SERVICES

def stop_owned(root,query=None,execute=None):
    import platform
    if platform.system()=='Linux':
        from linux_services import stop_unit
        names=['lan','web','channels','assistant','media','runner','emulator']
        return {name for name in names if stop_unit(root,name)}
    root=Path(root).resolve();jobs=owned_jobs(root,query)
    def run(args):
        if execute:execute(args)
        else:subprocess.run([str(x) for x in args],cwd=root,check=True)
    for label,_ in reversed(SERVICES):
        if label not in jobs or label.endswith('emulator.p2'):continue
        run(['launchctl','bootout',f'gui/{os.getuid()}/'+label])
    if 'dev.aiphone.emulator.p2' in jobs:run([root/'scripts/android-stop.sh'])
    print('Stopped verified owned services; disks, database and accounts preserved',flush=True)
    return set(jobs)

if __name__=='__main__':stop_owned(Path(__file__).resolve().parent.parent)
