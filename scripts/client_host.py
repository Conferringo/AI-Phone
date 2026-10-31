"""Managed host installation. No user-state replacement or implicit ownership adoption."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
import zipfile


def execute(args, **kwargs):
    return subprocess.run([str(a) for a in args], check=True, **kwargs)


def download(record, destination):
    destination = Path(destination); destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    if destination.is_file() and hashlib.sha256(destination.read_bytes()).hexdigest() == record['sha256']: return destination
    temp = destination.with_suffix('.partial')
    try:
        # Bootstrap requests are direct. Guest applications manage their own proxy.
        with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(record['url'], timeout=60) as response, temp.open('wb') as target:
            shutil.copyfileobj(response, target)
        if hashlib.sha256(temp.read_bytes()).hexdigest() != record['sha256']: raise ValueError('Downloaded dependency checksum mismatch')
        temp.replace(destination)
    finally: temp.unlink(missing_ok=True)
    return destination


def check_request(request):
    if not isinstance(request, dict) or set(request) != {'locale', 'phone', 'desktop'}: raise ValueError('Invalid local deployment request')
    from install_language import LOCALES
    if request['locale'] not in LOCALES: raise ValueError('Invalid interface language')
    for section, ram in [('phone', (1536, 2048, 3072, 4096)), ('desktop', (2048, 2560, 3072, 4096))]:
        item = request[section]
        expected = {'cpus', 'memory_mb', 'storage_gb'} if section == 'phone' else {'cpus', 'memory_mb'}
        if not isinstance(item, dict) or set(item) != expected or type(item['cpus']) is not int or item['cpus'] not in (1, 2, 4) or type(item['memory_mb']) is not int or item['memory_mb'] not in ram: raise ValueError('Invalid device resources')
    if type(request['phone']['storage_gb']) is not int or request['phone']['storage_gb'] not in (16, 24, 32): raise ValueError('Invalid phone storage')
    return request


def sdk_licenses(manager, env, emit, packages):
    """Forward each SDK prompt; never accept a license before a human response."""
    from android_sdk import command
    argv = command(manager, env, packages) if env else [str(manager), *packages]
    child = subprocess.Popen(argv, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=0)
    try:
        text = ''
        while char := child.stdout.read(1):
            text += char
            if len(text) > 128 * 1024: raise ValueError('SDK license output exceeded limit')
            if text.rstrip().endswith(('(y/N)?', '(y/N):', '(y/N)? ', '(y/N)')):
                emit('sdk-licenses', 'license', license=text)
                answer = sys.stdin.readline()
                if not answer or json.loads(answer) != {'accept': True}: raise ValueError('SDK license was not accepted')
                child.stdin.write('y\n'); child.stdin.flush(); text = ''
                emit('sdk-licenses')
        if child.wait() != 0:
            print(text[-4096:],file=sys.stderr)
            raise ValueError('SDK package installation or license review did not complete')
    finally:
        if child.poll() is None: child.terminate(); child.wait(timeout=5)
        child.stdout.close(); child.stdin.close()


def prepare_sdk(record, sdk, cache):
    if (sdk / 'cmdline-tools/latest/bin/sdkmanager').is_file(): return
    archive = download(record, cache / 'sdk.zip')
    with zipfile.ZipFile(archive) as source:
        for entry in source.infolist():
            path = PurePosixPath(entry.filename)
            if path.is_absolute() or '..' in path.parts or '\\' in entry.filename or not path.parts or path.parts[0] != 'cmdline-tools' or entry.external_attr >> 16 & 0o170000 == 0o120000: raise ValueError('Invalid SDK archive')
        folder = sdk / 'cmdline-tools'; folder.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='.sdk-', dir=folder) as temp:
            source.extractall(temp)
            for path in (Path(temp) / 'cmdline-tools/bin').iterdir(): path.chmod(0o755)
            (Path(temp) / 'cmdline-tools').rename(folder / 'latest')


def write_container_wrapper(root, home, command, state=None):
    bin_dir=home/'bin'
    if bin_dir.is_symlink():raise ValueError('Docker wrapper directory was replaced')
    bin_dir.mkdir(exist_ok=True)
    wrapper=bin_dir/'docker'
    # Compose runs in the owned VM. sudo must not erase the selected architecture
    # or resource profile; only these installation fields cross this boundary.
    defaults={'AI_PHONE_CLASH_DESKTOP_DIR':'linux','AI_PHONE_DESKTOP_CPUS':'2','AI_PHONE_DESKTOP_MEMORY':'2560m','AI_PHONE_CONTAINER_PROXY':'','AI_PHONE_POSTGRES_IMAGE':'','AI_PHONE_REDIS_IMAGE':'','AI_PHONE_DESKTOP_IMAGE':'','AI_PHONE_WORKSPACE_IMAGE':'','AI_PHONE_IMAGE_PULL_POLICY':'missing'}
    arguments=' '.join(key+'="${'+key+':-'+value+'}"' for key,value in defaults.items())
    text='#!/bin/sh\nexport LIMA_HOME='+shlex.quote(str(state or home/'lima'))+'\nexec '+shlex.quote(str(command))+' shell --workdir '+shlex.quote(str(root))+' ai-phone sudo env '+arguments+' docker "$@"\n'
    publish_container_wrapper(root,wrapper,text)
    return bin_dir


def publish_container_wrapper(root, wrapper, text):
    if wrapper.is_symlink():raise ValueError('Docker wrapper was replaced')
    if wrapper.exists() and wrapper.read_text()!=text:
        from managed_engine import selection
        if selection(root) is None:raise ValueError('Unowned Docker wrapper was preserved')
    with tempfile.NamedTemporaryFile(mode='w',prefix='.docker-',dir=wrapper.parent,delete=False) as output:
        temporary=Path(output.name)
        try:
            output.write(text);output.flush();temporary.chmod(0o755);temporary.replace(wrapper)
        finally:temporary.unlink(missing_ok=True)


def source_container_runtime(home, config, host):
    lima = home / 'tools/lima'
    command = lima / 'bin/limactl'
    if command.is_file():
        return command
    if lima.exists():
        raise ValueError('Incomplete Lima tools were preserved; review repair before continuing')
    archive = download(config['lima_hosts'][host['lima']], home / 'cache/lima.tar.gz')
    from container_host_runtime import unpack_lima
    lima.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.lima-', dir=lima.parent) as temporary:
        staged = Path(temporary) / 'runtime'
        unpack_lima(archive, staged)
        if not (staged / 'bin/limactl').is_file():
            raise ValueError('Lima executable is missing')
        staged.rename(lima)
    return command


def source_container_definition(home, config, host, memory):
    image = config['container_hosts'][host['image']]
    return {'vmType': host['vm_type'], 'arch': host['vm_arch'], 'cpus': 4, 'memory': memory, 'disk': '64GiB',
            'images': [{'location': image['url'], 'arch': host['vm_arch'], 'digest': 'sha256:' + image['sha256']}],
            'mountType': 'virtiofs', 'mounts': [{'location': str(home), 'writable': True}],
            'containerd': {'system': False, 'user': False}, 'propagateProxyEnv': False,
            'provision': [{'mode': 'system', 'script': '#!/bin/sh\nset -eu\napt-get update\nDEBIAN_FRONTEND=noninteractive apt-get install -y docker.io docker-compose-v2 docker-buildx\nsystemctl enable --now docker\nusermod -aG docker {{.User}}\n'}],
            'hostResolver': {'hosts': {'host.docker.internal': 'host.lima.internal'}},
            'portForwards': [{'guestPortRange': [17860, 17869], 'ignore': True}]}


def activate_container_engine(root, home, command, value):
    from managed_engine import state_directory, claim_state, save_selection
    state = state_directory(root, home)
    claim_state(root, state)
    os.environ['LIMA_HOME'] = str(state)
    definition = home / 'container-host.yaml'
    definition.write_text(json.dumps(value))
    # Listing all owned instances succeeds even before the first one exists.
    found = subprocess.run([str(command), 'list', '--format', '{{if eq .Name "ai-phone"}}{{.Status}}{{end}}'], capture_output=True, text=True)
    if found.returncode or found.stdout.strip() not in ('', 'Running', 'Stopped'):
        raise ValueError('Container host state is unknown; existing devices were preserved')
    execute([command, 'start', '--tty=false', 'ai-phone'] if found.stdout.strip() else [command, 'start', '--tty=false', '--name=ai-phone', definition])
    bin_dir = write_container_wrapper(root, home, command, state)
    os.environ['PATH'] = str(bin_dir) + os.pathsep + os.environ['PATH']
    execute(['docker', 'info'], stdout=subprocess.DEVNULL)
    save_selection(root, home, state, command, bin_dir)
    os.environ['AI_PHONE_MANAGED_BIN'] = str(bin_dir)


def container_engine(root, home, config, emit):
    """Reuse the selected engine or start the installation's authenticated host."""
    from host_platform import host_policy
    from container_host_runtime import selection, definition
    runtime = selection(root)
    if runtime is None and os.environ.get('AI_PHONE_REQUIRE_MANAGED_ENGINE') != '1' and shutil.which('docker') and subprocess.run(['docker', 'info'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
        return
    emit('container-host')
    _, host = host_policy(root)
    memory = str(max(4, (int(os.environ['AI_PHONE_DESKTOP_MEMORY'].rstrip('m')) + 2048 + 1023) // 1024)) + 'GiB'
    if runtime is not None:
        command, value = runtime['command'], definition(home, runtime, memory)
    else:
        command = source_container_runtime(home, config, host)
        value = source_container_definition(home, config, host, memory)
    activate_container_engine(root, home, command, value)


def verify_container_ownership(root, query=None):
    for name in ('ai-phone-postgres-1', 'ai-phone-redis-1', 'ai-phone-desktop'):
        result = query(name) if query else subprocess.run(['docker', 'inspect', '--format', '{{json .Config.Labels}}', name], capture_output=True, text=True)
        if result.returncode:
            # A missing container is fine; an unreachable daemon is not.
            if not query: execute(['docker', 'info'], stdout=subprocess.DEVNULL)
            continue
        labels = json.loads(result.stdout)
        if not isinstance(labels, dict) or labels.get('com.docker.compose.project') != 'ai-phone' or Path(labels.get('com.docker.compose.project.working_dir', '')).resolve() != root.resolve():
            from service_start import OwnershipConflict
            raise OwnershipConflict('Another installation owns existing AI Phone containers; no containers or data were changed')


def installation_reserve(root, home, config):
    admitted=False
    try:
        record=json.loads((home/'deployment.json').read_text())
        admitted=type(record.get('format')) is int and record['format']==1 and record.get('payload')==root.name and len(root.name)==64 and isinstance(record.get('progress',{}).get('state'),str) and bool(record['progress']['state'])
    except (OSError,ValueError,AttributeError):pass
    resumed=(root/'data/deployment/profile.json').is_file() or admitted
    return config['minimum_resume_free_gb' if resumed else 'minimum_free_gb']


def install_preflight(root, home, config, bundled_services, emit):
    if platform.system()=='Darwin':
        from service_start import owned_jobs
        owned_jobs(root)
    minimum=installation_reserve(root,home,config)
    if shutil.disk_usage(home).free<minimum*1024**3:
        raise ValueError(f'At least {minimum} GiB free space is required')
    if platform.system()=='Darwin' and not bundled_services:
        if subprocess.run(['xcode-select','-p'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode:
            emit('system-tools','blocked',error='command_line_tools')
            subprocess.run(['xcode-select','--install'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            return False
    if platform.system()=='Linux':
        from linux_host import prepare_system
        from prebuilt_services import load_manifest
        from container_host_runtime import selection
        bundled_ocr = bundled_services and load_manifest(root).get('vision') == 'linux-tesseract-bundled'
        prepare_system(emit,build_tools=not bundled_services,ocr_tools=not bundled_ocr,engine_tools=selection(root) is None)
    return True


def install_tools(home, mise, tools, emit):
    if not tools:return
    emit('toolchain')
    execute([mise,'install','-y',*tools],cwd=home)
    result=execute([mise,'env','--json',*tools],cwd=home,capture_output=True,text=True)
    os.environ.update(json.loads(result.stdout))


def prepare_profile(root, home, request, host, android_sdk=None):
    import release
    profile_path=root/'data/deployment/profile.json'
    if profile_path.exists():
        existing=json.loads(profile_path.read_text())
        release.validate(existing)
        if existing['phone']['template']!=host['phone_template']:
            raise ValueError('Existing phone architecture differs from this host; no devices were changed')
        return
    value=dict(schema_version=1,local_entry=True,locale=request['locale'],sdk_root=str(android_sdk or home/'tools/android-sdk'),java_home=os.environ.get('JAVA_HOME',''),host_proxy='',container_proxy='',
               phone={'template':host['phone_template'],**request['phone']},desktop={'template':'debian13-openbox',**request['desktop']})
    profile=home/'request-profile.json';profile.write_text(json.dumps(value));profile.chmod(0o600)
    release.configure(root,profile)


def deployment_sdk(root, provided):
    profile=root/'data/deployment/profile.json'
    return Path(json.loads(profile.read_text())['sdk_root']) if profile.exists() else provided


def selected_tools(tools, bundled_services, bundled_android, bundled_tools=(), bundled_phone_apps=False):
    excluded=set(bundled_tools)
    if bundled_services:excluded.add('rust')
    if bundled_android and bundled_phone_apps:excluded.add('java')
    return [tool for tool in tools if tool.split('@',1)[0] not in excluded]


def install(root, home, request, mise, emit, android_sdk=None, image_runtime=None, tool_runtime=None):
    request=check_request(request)
    from host_platform import host_policy
    from prebuilt_services import launch_spec
    config,host=host_policy(root)
    available=()
    if tool_runtime is not None:
        from installer_tools import select
        os.environ.update(select(root,tool_runtime));available=('python','node')
    bundled=launch_spec(root,'assistant')['prebuilt']
    if not install_preflight(root,home,config,bundled,emit):return
    from android_runtime import bundled_sdk
    android_sdk=deployment_sdk(root,android_sdk)
    android_bundled=android_sdk is not None and bundled_sdk(android_sdk) is not None
    from prebuilt_phone import available as phone_apps_available
    phone_apps=phone_apps_available(root)
    if android_bundled and not phone_apps:raise ValueError('Bundled Android needs prebuilt phone applications; no devices were changed')
    tools=selected_tools(config['tools'],bundled,android_bundled,available,phone_apps)
    if tools and mise is None:
        mise=download(config['mise'][host['mise']],home/'tools/bin/mise');mise.chmod(0o700)
    install_tools(home,mise,tools,emit)
    prepare_profile(root,home,request,host,android_sdk)
    from runtime_config import runtime_env
    os.environ.update(runtime_env(root))
    if not android_bundled:
        emit('sdk-tools')
        prepare_sdk(config['sdk_hosts'][host['sdk']],Path(os.environ['AI_PHONE_SDK_ROOT']),home/'cache')
    container_engine(root,home,config,emit)
    verify_container_ownership(root)
    if image_runtime is not None:
        from container_images import stage, activate
        emit('container-host')
        os.environ.update(activate(root,stage(image_runtime)))
    import release
    release.install(root=root,observer=lambda phase,state:emit(phase,state),license_action=lambda manager,env,packages:sdk_licenses(manager,env,emit,packages),launch_client=False,minimum_free_gb=config['minimum_resume_free_gb'])
    emit('devices')
    from prebuilt_js import launch_spec as js_launch_spec
    execute(js_launch_spec(root, 'device-setup')['argv'],cwd=root)
    emit('ready','ready')
