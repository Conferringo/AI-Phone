"""Start the sealed Linux backend inside its owned WSL distro; no native phone bridge."""
import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import time

BUNDLE=Path('/opt/aiphone/bundle')
TOOLS=Path('/opt/aiphone/tools')
INSTALLATION=Path('/var/lib/aiphone/installation')
SOCKET='unix:///run/aiphone/docker.sock'
FOREIGN_ENGINE=('DOCKER_HOST','DOCKER_CONFIG','DOCKER_CONTEXT','DOCKER_TLS','DOCKER_TLS_VERIFY','DOCKER_CERT_PATH','DOCKER_API_VERSION','LIMA_HOME','AI_PHONE_MANAGED_BIN')


def environment():
    inherited={key:value for key,value in os.environ.items() if key not in FOREIGN_ENGINE}
    return {**inherited,'DOCKER_HOST':SOCKET,'DOCKER_CONFIG':str(INSTALLATION.parent/'docker-cli'),'PATH':'/usr/local/bin:/usr/bin:/bin',
            'PYTHONDONTWRITEBYTECODE':'1','NEXT_TELEMETRY_DISABLED':'1'}


def bind_environment():
    selected=environment()
    for name in FOREIGN_ENGINE:os.environ.pop(name,None)
    os.environ.update(selected)


def require_owned_backend(root):
    record=root/'data/deployment/container-host.json'
    if record.exists() or record.is_symlink():raise ValueError('WSL cannot reuse a Lima-owned installation; existing data was preserved')


def staged(home):
    from client_host_entry import stage_source
    root=stage_source(BUNDLE/'source.payload',home)
    sys.path.insert(0,str(root/'scripts'))
    return root


def require_linux():
    if platform.system()!='Linux' or platform.machine()!='x86_64':raise ValueError('WSL backend requires Linux x64')


def verify():
    from check_native_services import verify as native
    from check_prebuilt_web import verify as web
    from container_images import stage
    from prebuilt_js import verify as js
    require_linux()
    with tempfile.TemporaryDirectory(prefix='aiphone-wsl-verify-') as temporary:
        root=staged(Path(temporary)/'installation')
        js(root,root)
        checks={'native':native(root,tool_runtime=TOOLS),'web':web(root),'js_integrity':True}
        images=stage(BUNDLE/'images.payload').manifest
        if images['architecture']!='amd64':raise ValueError('WSL images are not amd64')
        checks['image_roles']=sorted(images['images'])
    return {'offline_components':checks,'windows_boot_verified':False}


def wait_engine(run,deadline=30):
    until=time.monotonic()+deadline
    while time.monotonic()<until:
        if run(['docker','info'],check=False,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0:return
        time.sleep(.25)
    raise ValueError('Owned WSL engine did not become ready')


def start_engine(root,run):
    from linux_services import start_unit
    Path('/run/aiphone').mkdir(mode=0o700,exist_ok=True)
    command=['/usr/local/bin/dockerd','--host',SOCKET,'--data-root','/var/lib/aiphone/docker',
             '--exec-root','/run/aiphone/engine','--pidfile','/run/aiphone/dockerd.pid']
    start_unit(root,'wsl-engine',command,environment())
    wait_engine(run)


def start_backend(root,run):
    from client_host import verify_container_ownership
    from container_images import stage,activate
    from linux_host import private_token,start_web_service
    from linux_services import start_unit,unit_state
    from prebuilt_services import launch_spec
    from prebuilt_js import launch_spec as js_launch_spec
    from runtime_config import runtime_env
    require_owned_backend(root);bind_environment()
    verify_container_ownership(root)
    os.environ.update(activate(root,stage(BUNDLE/'images.payload')))
    os.environ.update(runtime_env(root))
    private_token(root,'assistant');private_token(root,'phone-runner');private_token(root,'channels')
    compose=['docker','compose','-f','compose.yaml','-f','compose.dev.yaml']
    run([*compose,'up','-d','--wait','--no-build','--pull','never','postgres','redis'])
    assistant=launch_spec(root,'assistant')
    if not unit_state(root,'assistant'):
        run([sys.executable,root/'scripts/database.py','prepare'])
        run([*assistant['argv'],'--migrate'])
    start_unit(root,'assistant',assistant['argv'],{**os.environ,**assistant['environment']})
    web_environment=dict(os.environ)
    def start(name,argv,extra_env=None):return start_unit(root,name,argv,{**web_environment,**(extra_env or {})})
    start_web_service(root,web_environment,run,start)
    for name in ('channels','media'):
        spec=js_launch_spec(root,name);start_unit(root,name,spec['argv'],{**os.environ,**spec['environment']})
    # Start the bundled desktop without rebuilding, pulling or touching the native Android guest.
    run(['docker','compose','-f','compose.desktop.yaml','up','-d','--wait','--no-build','--pull','never','desktop'])


def initialize_locked(request):
    from client_host import check_request,prepare_profile
    require_linux()
    request=check_request(json.loads(Path(request).read_text()))
    if shutil.disk_usage('/').free<4*1024**3:raise ValueError('Keep 4 GiB free for backend initialization')
    bind_environment();root=staged(INSTALLATION)
    if (root/'data/restore-quarantine.json').exists():raise ValueError('Restored state is quarantined')
    require_owned_backend(root)
    from installer_tools import select
    os.environ.update(select(root,TOOLS))
    # The bridge is deliberately absent from this component; it cannot emit two-device readiness.
    prepare_profile(root,INSTALLATION,request,{'phone_template':'android35-x86_64'},Path('/opt/aiphone/native-phone-bridge'))
    def run(argv,**kwargs):return subprocess.run(list(map(str,argv)),cwd=root,env=dict(os.environ),check=kwargs.pop('check',True),**kwargs)
    start_engine(root,run);start_backend(root,run)
    return {'backend_started':True,'phone_ready':False,'complete_device_bundle':False,'windows_boot_verified':False}


def initialize(request):
    from installer_lock import acquire
    INSTALLATION.parent.mkdir(parents=True,mode=0o700,exist_ok=True)
    with acquire(INSTALLATION.parent/'initialize.lock'):
        return initialize_locked(request)


def main(argv=None):
    sys.path.insert(0,str(Path(__file__).resolve().parent))
    parser=argparse.ArgumentParser(description=__doc__);group=parser.add_mutually_exclusive_group(required=True)
    group.add_argument('--verify',action='store_true');group.add_argument('--initialize',metavar='REQUEST')
    args=parser.parse_args(argv)
    print(json.dumps(verify() if args.verify else initialize(args.initialize)))


if __name__=='__main__':main()
