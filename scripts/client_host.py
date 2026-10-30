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


def sdk_licenses(manager, env, emit):
    """Forward each SDK prompt; never accept a license before a human response."""
    from android_sdk import command
    argv = command(manager, env, ['--licenses']) if env else [str(manager), '--licenses']
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
        if child.wait() != 0: raise ValueError('SDK licenses did not complete')
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


def container_engine(root, home, config, emit):
    """Use an existing engine, or an app-owned VZ VM mounting only app data."""
    if os.environ.get('AI_PHONE_REQUIRE_MANAGED_ENGINE')!='1' and shutil.which('docker') and subprocess.run(['docker', 'info'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
        return
    emit('container-host')
    from host_platform import host_policy
    _,host=host_policy(root)
    tools = home / 'tools'; lima = tools / 'lima'; command = lima / 'bin/limactl'
    if not command.is_file():
        archive = download(config['lima_hosts'][host['lima']], home / 'cache/lima.tar.gz')
        with tarfile.open(archive) as source:
            # Lima owns bin/share; reject link escapes before extraction.
            entries=[]
            for entry in source.getmembers():
                path = PurePosixPath(entry.name)
                # Upstream's documentation alias is unnecessary at runtime.
                if entry.issym() and entry.name == './share/doc/lima/templates' and entry.linkname == '../../lima/templates': continue
                if path.is_absolute() or '..' in path.parts or '\\' in entry.name or not (entry.isfile() or entry.isdir()): raise ValueError('Invalid Lima distribution')
                entries.append(entry)
            lima.mkdir(parents=True, exist_ok=True); source.extractall(lima, members=entries, filter='data')
    os.environ['LIMA_HOME'] = str(home / 'lima')
    image = config['container_hosts'][host['image']]
    memory = max(4, (int(os.environ['AI_PHONE_DESKTOP_MEMORY'].rstrip('m')) + 2048 + 1023) // 1024)
    value = {'vmType': host['vm_type'], 'arch': host['vm_arch'], 'cpus': 4, 'memory': f'{memory}GiB', 'disk': '64GiB',
             'images': [{'location': image['url'], 'arch': host['vm_arch'], 'digest': 'sha256:' + image['sha256']}],
             'mountType': 'virtiofs', 'mounts': [{'location': str(home), 'writable': True}],
             'containerd': {'system': False, 'user': False},
             'provision': [{'mode': 'system', 'script': '#!/bin/sh\nset -eu\napt-get update\nDEBIAN_FRONTEND=noninteractive apt-get install -y docker.io docker-compose-v2 docker-buildx\nsystemctl enable --now docker\nusermod -aG docker {{.User}}\n'}],
             'hostResolver': {'hosts': {'host.docker.internal': 'host.lima.internal'}},
             'portForwards': [{'guestPortRange': [17860, 17869], 'ignore': True}]}
    definition = home / 'container-host.yaml'; definition.write_text(json.dumps(value))
    # limactl start is idempotent for a running owned VM; no Docker context changes.
    found = subprocess.run([str(command), 'list', 'ai-phone', '--format', '{{.Status}}'], capture_output=True, text=True)
    execute([command, 'start', '--tty=false', 'ai-phone'] if found.stdout.strip() else [command, 'start', '--tty=false', '--name=ai-phone', definition])
    bin_dir = home / 'bin'; bin_dir.mkdir(exist_ok=True)
    wrapper = bin_dir / 'docker'
    wrapper.write_text('#!/bin/sh\nexport LIMA_HOME=' + shlex.quote(str(home / 'lima')) + '\nexec ' + shlex.quote(str(command)) + ' shell --workdir ' + shlex.quote(str(root)) + ' ai-phone sudo docker "$@"\n')
    wrapper.chmod(0o755); os.environ['PATH'] = str(bin_dir) + os.pathsep + os.environ['PATH']
    # Persist only owned tool paths; launchd jobs must retain the same engine.
    os.environ['AI_PHONE_MANAGED_BIN'] = str(bin_dir)
    execute(['docker', 'info'], stdout=subprocess.DEVNULL)


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


def install(root, home, request, mise, emit):
    request = check_request(request)
    from host_platform import host_policy
    config,host=host_policy(root)
    if platform.system()=='Darwin':
        from service_start import owned_jobs
        owned_jobs(root)  # Fail before installing if another checkout owns loaded jobs.
    minimum = installation_reserve(root, home, config)
    if shutil.disk_usage(home).free < minimum * 1024 ** 3: raise ValueError(f'At least {minimum} GiB free space is required')
    if platform.system()=='Darwin' and subprocess.run(['xcode-select', '-p'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
        emit('system-tools', 'blocked', error='command_line_tools')
        subprocess.run(['xcode-select', '--install'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return
    if platform.system()=='Linux':
        from linux_host import prepare_system
        prepare_system(emit)
    emit('toolchain')
    execute([mise, 'install', '-y', *config['tools']], cwd=home)
    result = execute([mise, 'env', '--json', *config['tools']], cwd=home, capture_output=True, text=True)
    os.environ.update(json.loads(result.stdout))
    # Every helper subprocess, including persisted launchd jobs, inherits owned tools.
    sdk = home / 'tools/android-sdk'
    profile_path = root / 'data/deployment/profile.json'
    if profile_path.exists():
        existing = json.loads(profile_path.read_text())
        # An upgrade may not silently change hardware or user configuration.
        from release import validate
        validate(existing)
        if existing['phone']['template']!=host['phone_template']:raise ValueError('Existing phone architecture differs from this host; no devices were changed')
    else:
        import release
        value = dict(schema_version=1, local_entry=True, locale=request['locale'], sdk_root=str(sdk), java_home=os.environ['JAVA_HOME'], host_proxy='', container_proxy='',
                     phone={'template': host['phone_template'], **request['phone']}, desktop={'template': 'debian13-openbox', **request['desktop']})
        profile = home / 'request-profile.json'; profile.write_text(json.dumps(value)); profile.chmod(0o600)
        release.configure(root, profile)
    from runtime_config import runtime_env
    os.environ.update(runtime_env(root))
    emit('sdk-tools'); prepare_sdk(config['sdk_hosts'][host['sdk']], Path(os.environ['AI_PHONE_SDK_ROOT']), home / 'cache')
    container_engine(root, home, config, emit)
    verify_container_ownership(root)
    import release
    release.ROOT = root
    # The initial reserve was checked before downloading tools. It is not a
    # permanently required unused reserve on every retry. Actual ENOSPC fails
    # the current checkpoint and never triggers deletion of user state.
    release.install(observer=lambda phase, state: emit(phase, state), license_action=lambda manager, env: sdk_licenses(manager, env, emit), launch_client=False, minimum_free_gb=config['minimum_resume_free_gb'])
    emit('devices')
    execute([root / 'scripts/bun', root / 'scripts/client-device-setup.ts'], cwd=root)
    emit('ready', 'ready')
