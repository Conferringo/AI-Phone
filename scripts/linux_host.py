"""Linux installation prerequisites and owned background service launch."""
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import sys
from client_host import execute
from linux_services import start_unit, unit_state
from runtime_config import runtime_env


class HostPrerequisite(ValueError):
    def __init__(self,code):
        self.install_code=code
        super().__init__(code)


def system_packages(*, build_tools=True):
    values={}
    for line in Path('/etc/os-release').read_text().splitlines():
        if '=' in line:
            key,value=line.split('=',1);values[key]=value.strip('"')
    distro=values.get('ID');version=int(values.get('VERSION_ID','0').split('.')[0])
    if distro not in ('ubuntu','debian'):raise HostPrerequisite('linux_distribution')
    packages=['ca-certificates','unzip','openssh-client','qemu-system-x86','qemu-utils','ovmf','virtiofsd','tesseract-ocr','tesseract-ocr-eng','tesseract-ocr-chi-sim','python3-pil','libgl1','libnss3','libx11-xcb1','libxcb-cursor0','libxkbcommon-x11-0','libpulse0','acl']
    if build_tools:packages.extend(['build-essential','pkg-config','libssl-dev'])
    packages.append('libasound2t64' if (distro=='ubuntu' and version>=24) or (distro=='debian' and version>=13) else 'libasound2')
    return packages


def install_dependencies(packages, emit):
    status=subprocess.run(['dpkg-query','-W','-f=${Package} ${Status}\n',*packages],capture_output=True,text=True)
    installed={line.split()[0] for line in status.stdout.splitlines() if line.endswith('install ok installed')}
    if set(packages)<=installed:return
    emit('system-tools')
    prefix=[] if os.geteuid()==0 else ['pkexec']
    if prefix and not shutil.which('pkexec'):raise HostPrerequisite('linux_authorization')
    try:
        execute([*prefix,'apt-get','update'])
        execute([*prefix,'apt-get','install','-y','--no-install-recommends',*packages])
    except subprocess.CalledProcessError as error:
        if error.returncode in (126,127):raise HostPrerequisite('linux_authorization') from error
        raise ValueError('Linux dependency installation failed; inspect the private installer log and retry') from error


def require_kvm():
    kvm=Path('/dev/kvm')
    if not kvm.exists():raise HostPrerequisite('virtualization_unavailable')
    if not os.access(kvm,os.R_OK|os.W_OK):
        if not shutil.which('pkexec') or not shutil.which('setfacl'):raise HostPrerequisite('linux_authorization')
        # Access to the current KVM device only; no groups, global udev rules or Docker socket grants.
        execute(['pkexec','setfacl','-m','u:'+str(os.getuid())+':rw',str(kvm)])
    import fcntl
    try:
        with kvm.open('rb+') as device:
            if fcntl.ioctl(device,0xAE00,0)!=12:raise HostPrerequisite('virtualization_unavailable')
    except OSError as error:raise HostPrerequisite('virtualization_unavailable') from error


def prepare_system(emit, *, build_tools=True):
    """Use the OS authorization dialog for fixed distro dependencies, never passwords."""
    os.environ['PATH']=os.environ.get('PATH','/usr/bin:/bin')+':/usr/libexec'
    # Reject an absent accelerator before requesting package-install authorization.
    if not Path('/dev/kvm').exists():raise HostPrerequisite('virtualization_unavailable')
    install_dependencies(system_packages(build_tools=build_tools),emit)
    require_kvm()


def private_token(root, section):
    folder=root/'data'/section;folder.mkdir(parents=True,exist_ok=True,mode=0o700)
    token=folder/'token'
    if not token.exists():
        with os.fdopen(os.open(token,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600),'w') as stream:stream.write(secrets.token_hex(32))


def start_guest(root, child_env, run, start):
    run([root/'scripts/android-adb','start-server'])
    run(['sh',root/'scripts/android-create.sh'])
    created=start('emulator',['sh',str(root/'scripts/android-emulator.sh')])
    run(['sh',root/'scripts/android-ready.sh',*([] if created else ['--observe-only'])])
    for package,script in [('dev.aiphone.input','android-install-input.sh'),('dev.aiphone.home','android-install-home.sh'),('com.github.metacubex.clash.meta','android-install-clash.sh')]:
        result=subprocess.run([root/'scripts/android-adb','shell','pm','path',package],capture_output=True,text=True,env=child_env)
        if result.returncode or not result.stdout.startswith('package:'):run(['sh',root/'scripts'/script])


def start_execution_services(root, specs, run, start):
    if not unit_state(root,'runner') and not specs['runner']['prebuilt']:
        run(['sh',root/'scripts/build-phone-vision.sh']);run(['cargo','build','-p','ai-phone-runner','--locked'])
    start('runner',specs['runner']['argv'],specs['runner']['environment'])
    start('media',[str(root/'.tools/bin/bun'),str(root/'apps/device-stream/server.ts')])
    if not unit_state(root,'assistant'):
        run(['docker','compose','-f','compose.yaml','-f','compose.dev.yaml','up','-d','--wait','postgres','redis'])
        if not specs['assistant']['prebuilt']:run(['cargo','build','-p','ai-phone-assistant','--locked'])
        run([sys.executable,root/'scripts/database.py','prepare'])
        run([*specs['assistant']['argv'],'--migrate'])
    start('assistant',specs['assistant']['argv'],specs['assistant']['environment'])
    start('channels',[str(root/'.tools/bin/bun'),str(root/'apps/channels/server.ts')])


def start_workbench_services(root, env, run, start):
    from prebuilt_web import web_command
    prebuilt=(root/'runtime/manifest.json').is_file()
    if not prebuilt and not unit_state(root,'web'):run([root/'scripts/bun','run','build'])
    node=shutil.which('node')
    if not node:
        raise ValueError('Owned Node runtime unavailable')
    # Working directory belongs to the unit owner, so Next receives an explicit project path.
    if prebuilt:env.update({'PORT':'17860','HOSTNAME':'127.0.0.1'})
    start('web',web_command(root) if prebuilt else [node,str(root/'apps/web/node_modules/next/dist/bin/next'),'start',str(root/'apps/web'),'--hostname','127.0.0.1','--port','17860'])
    start('lan',[node,str(root/'scripts/lan-server.mjs')])


def start_missing(root):
    """Do not compile over a live executable/build or replace an active unit."""
    root=Path(root).resolve()
    if (root/'data/restore-quarantine.json').exists():
        raise ValueError('Restored state is quarantined; no services started')
    from prebuilt_services import launch_spec
    runner_spec=launch_spec(root,'runner');assistant_spec=launch_spec(root,'assistant')
    env={**runtime_env(root),'NEXT_TELEMETRY_DISABLED':'1'}
    # The private tool environment is inherited by the supervisor at spawn, never serialized wholesale.
    env['PATH']=os.environ.get('PATH',env['PATH'])
    child_env={**os.environ,**env};started=[]
    def run(args):execute(args,cwd=root,env=child_env)
    def start(name,argv,extra_env=None):
        created=start_unit(root,name,argv,{**env,**(extra_env or {})})
        if created:started.append(name)
        return created
    start_guest(root,child_env,run,start)
    for section in ['phone-runner','assistant','channels']:private_token(root,section)
    start_execution_services(root,{'runner':runner_spec,'assistant':assistant_spec},run,start)
    start_workbench_services(root,env,run,start)
    print('Started missing owned Linux services: '+(', '.join(started) or 'none'),flush=True)
    return started
