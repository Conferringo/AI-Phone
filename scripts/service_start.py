"""Resume only missing, owned macOS services; never restart a running user task."""
import os
import fcntl
from pathlib import Path
import re
import subprocess
from runtime_config import runtime_env

SERVICES=[('dev.aiphone.emulator.p2','android-start.sh'),('dev.aiphone.runner.p2','phone-runner-start.sh'),('dev.aiphone.device-stream','device-stream-start.sh'),('dev.aiphone.assistant','assistant-start.sh'),('dev.aiphone.channels','channels-start.sh'),('dev.aiphone.web.p2','phone-web-start.sh'),('dev.aiphone.lan','lan-start.sh')]

class OwnershipConflict(ValueError):
    install_code = 'foreign_installation'

def owned_jobs(root,query=None):
    root=Path(root).resolve();jobs={}
    for label,_ in SERVICES:
        result=query(label) if query else subprocess.run(['launchctl','print',f'gui/{os.getuid()}/'+label],capture_output=True,text=True)
        if result.returncode:continue
        match=re.search(r'^\s*working directory = (.+)$',result.stdout,re.M)
        if not match or Path(match[1]).resolve() not in (root,root/'apps/web'):
            raise OwnershipConflict('Another checkout owns '+label+'; no services were changed')
        state=re.search(r'^\s*state = (\S+)',result.stdout,re.M)
        jobs[label]=bool(state and state[1]=='running')
    return jobs

def _start_missing(root,query=None,execute=None,package_installed=None):
    root=Path(root).resolve()
    if (root/'data/restore-quarantine.json').exists():raise ValueError('Restored state is quarantined; no services started')
    jobs=owned_jobs(root,query)
    from managed_engine import ensure_running
    ensure_running(root)
    env={**os.environ,**runtime_env(root),'NEXT_TELEMETRY_DISABLED':'1'}
    started=[]
    for label,running in jobs.items():
        if not running:
            # No -k: if launchd started it since our observation, preserve that process.
            subprocess.run(['launchctl','kickstart',f'gui/{os.getuid()}/'+label],check=True)
            started.append(label)
    def run(script):
        if execute:execute(script)
        else:subprocess.run([root/'scripts'/script],cwd=root,env=env,check=True)
    def installed(package):
        if package_installed:return package_installed(package)
        r=subprocess.run([root/'scripts/android-adb','shell','pm','path',package],capture_output=True,text=True,env=env,timeout=15)
        return r.returncode==0 and r.stdout.strip().startswith('package:')
    ready=False
    if 'dev.aiphone.emulator.p2' not in jobs:
        run('android-create.sh');run('android-start.sh');run('android-ready.sh');ready=True
    # A killed installer may have booted Android but not installed its input/launcher.
    # Install only missing packages; never replace the live home screen or input setup.
    for package,script in [('dev.aiphone.input','android-install-input.sh'),('dev.aiphone.home','android-install-home.sh'),('com.github.metacubex.clash.meta','android-install-clash.sh')]:
        if not installed(package):
            if not ready:run('android-ready.sh');ready=True
            if not installed(package):run(script)
    for label,script in SERVICES:
        if label.endswith('emulator.p2') or label in jobs:continue
        run(script);started.append(label)
    print('Started missing services: '+(', '.join(started) if started else 'none')+'; existing tasks preserved',flush=True)
    return started

def start_missing(root,query=None,execute=None,package_installed=None):
    root=Path(root).resolve();folder=root/'data/deployment';folder.mkdir(parents=True,exist_ok=True,mode=0o700)
    fd=os.open(folder/'service-start.lock',os.O_RDWR|os.O_CREAT,0o600);os.fchmod(fd,0o600)
    with os.fdopen(fd,'a+') as lock:
        try:fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:raise ValueError('Another startup is in progress for this user directory')
        import platform
        if platform.system()=='Linux':
            from linux_host import start_missing as linux_start
            return linux_start(root)
        return _start_missing(root,query,execute,package_installed)

if __name__=='__main__':start_missing(Path(__file__).resolve().parent.parent)
