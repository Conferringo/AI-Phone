"""Fresh isolated CI deployment through the same worker used by the native client.

Only the CI image's already accepted SDK license files are copied. No SDK terms
are accepted by this script, and no model/account/chat fixtures are imported.
"""
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import urllib.request
from prepare_client_host import prepare
from client_host_entry import stage_source
from client_host import download


def directory_identity(path, *, owned=False):
    info = path.lstat()
    if not stat.S_ISDIR(info.st_mode) or (owned and info.st_uid != os.getuid()):
        raise ValueError('CI fixture directory identity could not be verified')
    return info.st_dev, info.st_ino


def verify_fixture_root(home, identity):
    if sys.platform != 'linux' or os.environ.get('GITHUB_ACTIONS') != 'true' or os.environ.get('RUNNER_OS') != 'Linux':
        raise ValueError('Fixture cleanup is restricted to an ephemeral Linux CI runner')
    if not home.is_absolute() or home.parent.resolve() != Path(tempfile.gettempdir()).resolve() or not home.name.startswith('ai-phone-host-acceptance-'):
        raise ValueError('CI fixture root differs from its TemporaryDirectory')
    if directory_identity(home, owned=True) != identity:
        raise ValueError('CI fixture root identity changed')


def fixture_release_paths(home, staged):
    if staged.parent != home / 'releases' or re.fullmatch('[0-9a-f]{64}', staged.name) is None:
        raise ValueError('CI fixture source is outside its fixed release layout')
    paths = [home, home / 'releases', staged, home / 'user', home / 'user/data']
    pinned = [(path, directory_identity(path, owned=True)) for path in paths]
    if not (staged / 'data').is_symlink() or (staged / 'data').resolve(strict=True) != (home / 'user/data').resolve(strict=True):
        raise ValueError('CI fixture persistent data link changed')
    return pinned


def fixture_paths(home, staged, identity):
    home, staged = Path(home), Path(staged)
    verify_fixture_root(home, identity)
    pinned = fixture_release_paths(home, staged)
    guest = home / 'user/data/desktop/home'
    for path in (guest.parent, guest):
        if path.exists() or path.is_symlink():
            pinned.append((path, directory_identity(path)))
    return guest, pinned


def desktop_home_mount(mounts):
    if not isinstance(mounts, list) or not all(isinstance(mount, dict) for mount in mounts):
        raise ValueError('CI desktop bind could not be verified')
    homes = [mount for mount in mounts if mount.get('Destination') == '/home/ai']
    if len(homes) != 1:
        raise ValueError('CI desktop must have exactly one home bind')
    return homes[0]


def verify_fixture_bind(run, identity, guest, staged):
    result = run(['docker', 'container', 'inspect', '--format', '{{json .Mounts}}', identity],
                 capture_output=True, text=True, check=True)
    if len(result.stdout) > 16384:
        raise ValueError('CI desktop bind response exceeds its budget')
    home = desktop_home_mount(json.loads(result.stdout))
    source = home.get('Source')
    if home.get('Type') != 'bind' or home.get('RW') is not True or source not in (str(guest.resolve(strict=True)), str(staged / 'data/desktop/home')):
        raise ValueError('CI desktop home belongs to a different fixture')
    if Path(source).resolve(strict=True) != guest.resolve(strict=True):
        raise ValueError('CI desktop home belongs to a different fixture')


def cleanup_owned_fixture(home, staged, base_env, identity):
    guest, pinned = fixture_paths(home, staged, identity)
    from runtime_config import runtime_env
    from client_host import verify_container_ownership
    env = {**base_env, **runtime_env(staged)}

    def run(argv, **options):
        return subprocess.run(list(map(str, argv)), cwd=staged, env=env, timeout=90, **options)

    containers = verify_container_ownership(staged, query=run)
    desktop = containers.get('ai-phone-desktop')
    if desktop is not None:
        verify_fixture_bind(run, desktop['id'], guest, staged)
    # Every container and bind is verified before stopping any owned process.
    run([sys.executable, staged / 'scripts/service_stop.py'], check=True)
    for container in containers.values():
        run(['docker', 'stop', '--time', '20', container['id']], check=True)
        run(['docker', 'rm', container['id']], check=True)
    if fixture_paths(home, staged, identity) != (guest, pinned):
        raise ValueError('CI fixture paths changed during shutdown')
    if desktop is not None:
        # Only this fixture's fixed bind home; no production data repair or volume removal.
        run(['sudo', '-n', 'rm', '--one-file-system', '--recursive', '--force', '--', guest], check=True)


def cleanup_fixture(home, staged, base_env, identity, *, failure=None):
    try:
        cleanup_owned_fixture(home, staged, base_env, identity)
    except BaseException as cleanup_error:
        if failure is not None:
            raise BaseExceptionGroup('Deployment and fixture cleanup both failed',
                                     [failure, cleanup_error]) from None
        raise


def require_ci_primary_group():
    """Read-only CI contract: KVM access must not replace the passwd primary group."""
    import grp
    import pwd
    uid = os.getuid()
    if uid == 0 or os.geteuid() != uid:
        raise ValueError('CI deployment requires the original non-root UID')
    try:
        account = pwd.getpwuid(uid)
        kvm = grp.getgrnam('kvm').gr_gid
        groups = os.getgroups()
    except (KeyError, OSError) as error:
        raise ValueError('CI user and KVM group could not be verified') from error
    if account.pw_uid != uid or os.getgid() != account.pw_gid or os.getegid() != account.pw_gid:
        raise ValueError('CI deployment requires its passwd identity and primary group')
    if kvm != account.pw_gid and kvm not in groups:
        raise ValueError('CI deployment requires KVM group access')


def main():
    if os.environ.get('GITHUB_ACTIONS')!='true' or os.environ.get('RUNNER_OS')!='Linux':raise ValueError('Fresh-host acceptance is restricted to an ephemeral Linux CI runner')
    require_ci_primary_group()
    root=Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix='ai-phone-host-acceptance-') as temp:
        home=Path(temp);home_info=home.lstat();identity=(home_info.st_dev,home_info.st_ino)
        config=json.loads((root/'config/host-bootstrap.json').read_text());prepare(root)
        payload=root/'apps/client/src-tauri/gen/host/source.payload'
        staged=stage_source(payload,home)
        sdk=home/'tools/android-sdk';licenses=Path(os.environ['ANDROID_HOME'])/'licenses'
        if not licenses.is_dir():raise ValueError('The CI image has no prior SDK license acceptance')
        shutil.copytree(licenses,sdk/'licenses')
        request=home/'request.json';request.write_text(json.dumps({'locale':'en','phone':{'cpus':2,'memory_mb':2048,'storage_gb':16},'desktop':{'cpus':2,'memory_mb':2048}}))
        mise=download(config['mise']['linux-x64'],home/'tools/bin/mise');mise.chmod(0o700)
        env={**os.environ,**{key:str(home/'tools'/value) for key,value in {'MISE_DATA_DIR':'mise/data','MISE_CACHE_DIR':'mise/cache','MISE_CONFIG_DIR':'mise/config','CARGO_HOME':'cargo','RUSTUP_HOME':'rustup'}.items()},'MISE_YES':'1','MISE_PARANOID':'1','CARGO_BUILD_JOBS':'2'}
        command=[str(mise),'exec','-y','python@3.13.5','--','python',str(root/'scripts/client_host_entry.py'),'--payload',str(payload),'--home',str(home),'--request',str(request),'--mise',str(mise)]
        try:
            # Empty stdin fails on an unreviewed new SDK license instead of accepting it.
            subprocess.run(command,stdin=subprocess.DEVNULL,env=env,check=True,timeout=3000)
            request=urllib.request.Request('http://127.0.0.1:17860/api/setup/status',method='POST',data=b'',headers={'Origin':'http://127.0.0.1:17860','Content-Type':'application/x-protobuf'})
            with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request,timeout=8) as response:
                if response.status!=200 or response.headers.get('Content-Type')!='application/x-protobuf':raise ValueError('Fresh setup endpoint not usable')
            # The worker's ready event already requires a real root phone PNG and desktop PNG.
            print('LINUX_HOST_ACCEPTANCE: real root phone, desktop frame, local setup handoff',flush=True)
            sys.path.insert(0,str(staged/'scripts'))
            from linux_services import unit_state
            before={name:unit_state(staged,name)['pid'] for name in ['emulator','runner','media','assistant','channels','web','lan']}
            subprocess.run(command,stdin=subprocess.DEVNULL,env=env,check=True,timeout=300)
            after={name:unit_state(staged,name)['pid'] for name in before}
            if before!=after:raise ValueError('Resume replaced an already running owned service')
            print('LINUX_HOST_RESUME: all seven supervisor PIDs preserved',flush=True)
        finally:
            cleanup_fixture(home,staged,env,identity,failure=sys.exception())


if __name__=='__main__':main()
