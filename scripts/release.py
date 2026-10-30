"""Guided source-preview installation; never deletes devices or resets accounts."""
import argparse
import json
import os
from pathlib import Path
import platform
import secrets
import shlex
import shutil
import subprocess
import sys
import tempfile
import urllib.request
from runtime_config import runtime_env
from install_language import LOCALES, CATALOGS, choose_language

ROOT = Path(__file__).resolve().parent.parent
PROFILE = 'data/deployment/profile.json'


def run(args, root=ROOT, env=None):
    subprocess.run([str(a) for a in args], cwd=root, env=env, check=True)


def validate(value):
    if not isinstance(value,dict): raise ValueError('Profile must be a JSON object')
    for section in ('phone','desktop'):
        if not isinstance(value.get(section),dict) or not all(k in value[section] for k in ('template','cpus','memory_mb')):
            raise ValueError('Profile missing device fields: '+section)
    if 'storage_gb' not in value['phone']: raise ValueError('Phone storage missing')
    for key in ('sdk_root','java_home','host_proxy','container_proxy','locale'):
        if not isinstance(value.get(key),str): raise ValueError('Profile missing string: '+key)
    if 'local_entry' in value and type(value['local_entry']) is not bool: raise ValueError('local_entry must be a boolean')
    if value.get('schema_version') != 1:
        raise ValueError('Unsupported profile schema')
    if value.get('locale') not in LOCALES:
        raise ValueError('Unsupported interface language')
    if value['phone'].get('template') not in ('android35-arm64','android35-x86_64') or value['desktop'].get('template') != 'debian13-openbox':
        raise ValueError('Only the Android 15 / Debian 13 templates are available')
    for section in ('phone', 'desktop'):
        p = value[section]
        if type(p['cpus']) is not int or p['cpus'] not in (1, 2, 4):
            raise ValueError('CPU must be 1, 2 or 4')
        memory_presets=(1536,2048,3072,4096) if section=='phone' else (2048,2560,3072,4096)
        if type(p['memory_mb']) is not int or p['memory_mb'] not in memory_presets:
            raise ValueError('Unsupported memory preset')
    if value['phone']['storage_gb'] not in (16, 24, 32):
        raise ValueError('Phone storage must be 16, 24 or 32 GB')
    for key in ('sdk_root', 'java_home'):
        if key=='java_home' and not value[key]:continue
        if not Path(value[key]).is_absolute() or any(ord(c) < 32 for c in value[key]):
            raise ValueError(f'{key} must be an absolute path')
    from urllib.parse import urlparse
    for key in ('host_proxy', 'container_proxy'):
        if value[key]:
            url = urlparse(value[key])
            if url.scheme not in ('http', 'https') or not url.hostname or url.username or url.password or url.path not in ('', '/') or url.query or url.fragment:
                raise ValueError('Use an HTTP(S) proxy URL without credentials or path')
    return value


def default_profile(root=ROOT):
    env = runtime_env(root)
    return dict(schema_version=1, local_entry=True, locale='zh-CN', sdk_root=env['AI_PHONE_SDK_ROOT'],
                java_home=env['JAVA_HOME'], host_proxy=env['AI_PHONE_HOST_PROXY'],
                container_proxy=env['AI_PHONE_CONTAINER_PROXY'],
                phone=dict(template='android35-x86_64' if env['AI_PHONE_ANDROID_ABI']=='x86_64' else 'android35-arm64', cpus=4, memory_mb=3072, storage_gb=16),
                desktop=dict(template='debian13-openbox', cpus=2, memory_mb=2560))


def configure(root=ROOT, source=None):
    destination = root / PROFILE
    value = json.loads(Path(source).read_text()) if source else (json.loads(destination.read_text()) if destination.exists() else default_profile(root))
    if not source:
        value['locale']=choose_language(value.get('locale','zh-CN'))
        labels=CATALOGS[value['locale']]
        print(labels['heading'])
        for key, label in [('sdk_root', 'Android SDK'), ('java_home', 'Java 17 home'), ('host_proxy', labels['host_proxy']), ('container_proxy', labels['container_proxy'])]:
            answer = input(f'{label} [{value[key]}]: ').strip()
            if answer:
                value[key] = '' if answer == '-' else answer
        for section in ('phone', 'desktop'):
            for key in ('cpus', 'memory_mb') + (('storage_gb',) if section == 'phone' else ()):
                answer = input(f'{labels[section]} · {labels[key]} [{value[section][key]}]: ').strip()
                if answer:
                    value[section][key] = int(answer)
    validate(value)
    destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, temp = tempfile.mkstemp(dir=destination.parent)
    try:
        with os.fdopen(fd, 'w') as f:
            json.dump(value, f, ensure_ascii=False, indent=2)
            f.write('\n'); f.flush(); os.fsync(f.fileno())
        os.replace(temp, destination)
    finally:
        if os.path.exists(temp): os.unlink(temp)
    # Never overwrite the current database password or account.
    env = root / '.env'
    if not env.exists():
        # Managed release links persist the database password across versions.
        env = env.resolve() if env.is_symlink() else env
        with os.fdopen(os.open(env, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'w') as f:
            f.write('POSTGRES_DB=ai_phone\nPOSTGRES_USER=ai_phone\nPOSTGRES_PASSWORD=' + secrets.token_hex(24) + '\n')
    print(CATALOGS[value['locale']]['saved'])


def check_result(name, passed, fix):
    return dict(name=name, passed=bool(passed), fix='' if passed else fix)


def host_checks(root):
    from host_platform import host_policy
    try:
        host_policy(root)
        supported=True
    except ValueError:
        supported=False
    return [check_result('host',supported,'Independent deployment currently requires macOS ARM64 or Linux x64'),
            check_result('python-version',sys.version_info>=(3,11),'Python 3.11+ required')]


def tool_checks(bundled_services):
    tools=[('node','Install Node.js 22+'),('docker','Prepare the managed container host')]
    if not bundled_services:tools.append(('cargo','Install Rust via rustup'))
    if platform.system()=='Darwin':
        if not bundled_services:tools.append(('swiftc','Install Xcode Command Line Tools'))
    else:
        tools.append(('tesseract','Install the managed Linux OCR dependencies'))
    return [check_result(name,shutil.which(name),hint) for name,hint in tools]


def node_checks():
    if not shutil.which('node'):return []
    result=subprocess.run(['node','-p','Number(process.versions.node.split(".")[0])>=22'],capture_output=True,text=True)
    return [check_result('node-version',result.returncode==0 and result.stdout.strip()=='true','Node.js 22+ required')]


def sdk_checks(env):
    sdk=Path(env['AI_PHONE_SDK_ROOT'])
    from android_runtime import bundled_sdk
    if bundled_sdk(sdk) is not None:return [check_result('android-runtime',True,'')]
    checks=[check_result('sdk/'+name,(sdk/name).is_file(),'Install Android command-line tools; configure SDK path')
            for name in ('platform-tools/adb','emulator/emulator','cmdline-tools/latest/bin/sdkmanager')]
    java=Path(env['JAVA_HOME'])/'bin/javac' if env['JAVA_HOME'] else None
    checks.append(check_result('java17',java is not None and java.is_file(),'Configure a Java 17 JDK home'))
    if java is not None and java.is_file():
        import re
        result=subprocess.run([str(java),'-version'],capture_output=True,text=True,timeout=10)
        version=re.search(r'javac (\d+)',result.stdout+result.stderr)
        checks.append(check_result('java-version',result.returncode==0 and version and int(version[1])>=17,'Java 17+ required'))
    return checks


def rust_checks(bundled_services):
    if bundled_services:
        return [check_result('native-services',True,'')]
    if not shutil.which('rustc'):
        return [check_result('rust-version',False,'Install the Rust compiler through rustup')]
    import re
    result=subprocess.run(['rustc','--version'],capture_output=True,text=True,timeout=10)
    version=re.search(r'rustc (\d+)\.(\d+)',result.stdout)
    return [check_result('rust-version',result.returncode==0 and version and tuple(map(int,version.groups()))>=(1,88),'Rust 1.88+ required')]


def docker_checks():
    if not shutil.which('docker'):return []
    engine=subprocess.run(['docker','info','--format','{{.OSType}}'],capture_output=True,text=True,timeout=15)
    compose=subprocess.run(['docker','compose','version'],capture_output=True,timeout=10)
    return [check_result('docker-engine',engine.returncode==0 and engine.stdout.strip()=='linux','Start Docker Desktop with Linux containers'),
            check_result('docker-compose',compose.returncode==0,'Install Docker Compose v2')]


def capacity_checks(root, minimum_free_gb):
    checks=[]
    if platform.system()=='Darwin':
        memory=int(subprocess.check_output(['sysctl','-n','hw.memsize'],text=True).strip())
        checks.append(check_result('memory-budget',memory>=12*1024**3,'12 GB host RAM minimum; 16 GB+ recommended for phone + desktop'))
    elif platform.system()=='Linux':
        memory=next(int(line.split()[1])*1024 for line in Path('/proc/meminfo').read_text().splitlines() if line.startswith('MemTotal:'))
        checks.append(check_result('memory-budget',memory>=12*1024**3,'12 GB host RAM minimum; 16 GB+ recommended'))
    checks.append(check_result('free-disk',shutil.disk_usage(root).free>=minimum_free_gb*1024**3,f'Keep at least {minimum_free_gb} GB free for SDK, images and builds'))
    return checks


def doctor(root=ROOT, *, minimum_free_gb=35):
    from prebuilt_services import launch_spec
    bundled=launch_spec(root,'assistant')['prebuilt']
    return [*host_checks(root),*tool_checks(bundled),*node_checks(),*sdk_checks(runtime_env(root)),
            *rust_checks(bundled),*docker_checks(),*capacity_checks(root,minimum_free_gb)]


def environment(root=ROOT):
    return {**os.environ, **runtime_env(root), 'NEXT_TELEMETRY_DISABLED': '1'}


def image_id(tag):
    result = subprocess.run(['docker', 'image', 'inspect', '--format', '{{.Id}}', tag], capture_output=True, text=True)
    return result.stdout.strip() if result.returncode == 0 else ''


def native_apps(root, env, download=False):
    operation = 'download' if download else 'verify'
    from prebuilt_js import launch_spec
    result = subprocess.run(launch_spec(root, 'native-apps')['argv'],
                            input=json.dumps({'operation': operation}), text=True, cwd=root, env=env, capture_output=True)
    if download and result.returncode:
        raise ValueError('Clash application download failed; check the installation network or AI_PHONE_BOOTSTRAP_PROXY')
    return result.returncode == 0


def bootstrap_valid(root):
    from prebuilt_js import available, private_bun
    if available(root):
        return private_bun(root) is not None
    if not (root/'.tools/bin/bun').is_file() or not (root/'apps/web/node_modules/next/package.json').is_file():
        return False
    result = subprocess.run([root/'.tools/bin/bun', '--version'], capture_output=True, text=True)
    return result.returncode == 0 and result.stdout.strip() == '1.4.4' and (root/'packages/protocol/src/gen/ai_phone/v1/system_pb.ts').is_file()


def messaging_browser(root, env, download=False):
    # Contributor bootstrap. Complete distributions must include this component.
    from prebuilt_js import launch_spec
    args = launch_spec(root, 'messaging-browser')['argv'] + ([] if download else ['--verify'])
    result = subprocess.run(args, cwd=root, env=env, capture_output=True, text=True)
    if download and result.returncode:
        raise ValueError('WhatsApp browser component preparation failed; check the installation network or provide AI_PHONE_WHATSAPP_CHROMIUM')
    return result.returncode == 0


def install_sdk(state, root, env, license_action):
    from install_state import fingerprint
    from android_sdk import command as sdk_command
    sdk = Path(env['AI_PHONE_SDK_ROOT']); abi = env['AI_PHONE_ANDROID_ABI']
    from android_runtime import bundled_sdk
    component=bundled_sdk(sdk)
    if component is not None:
        if component['abi']!=abi:raise ValueError('Bundled Android ABI differs from the device profile')
        state.phase('sdk-packages', fingerprint(root, [], {'sdk':str(sdk),'files':component['files']}),lambda:None,lambda:bundled_sdk(sdk) is not None)
        return
    manager = sdk/'cmdline-tools/latest/bin/sdkmanager'
    required = ('platform-tools/adb', 'emulator/lib/emulator_controller.proto', 'build-tools/35.0.0/d8',
                'build-tools/35.0.0/aapt2', 'platforms/android-35/android.jar', f'system-images/android-35/default/{abi}/system.img')
    # Accept only the terms for the actual packages, rather than unrelated SDKs.
    packages = ['platform-tools', 'emulator', 'platforms;android-35', 'build-tools;35.0.0', f'system-images;android-35;default;{abi}']
    state.phase('sdk-packages', fingerprint(root, [], {'sdk': str(sdk), 'api': 35, 'abi': abi}),
                lambda: license_action(manager, env, packages) if license_action else run(sdk_command(manager, env, packages), env=env, root=root),
                lambda: all((sdk/name).is_file() for name in required))


def install_source_components(state, root, env):
    from install_state import fingerprint
    from prebuilt_js import available, private_bun
    inputs = ['scripts/bootstrap.sh', 'package.json', 'bun.lock', 'buf.gen.yaml', 'packages/protocol/proto',
              *[str(p.relative_to(root)) for base in ('apps', 'packages') for p in (root/base).glob('*/package.json')]]
    prepare = (lambda: private_bun(root, create=True)) if available(root) else (lambda: run(['./scripts/bootstrap.sh'], env=env, root=root))
    state.phase('dependencies', fingerprint(root, inputs), prepare, lambda: bootstrap_valid(root))
    state.phase('native-apps', fingerprint(root, ['config/clash-apps.json', 'apps/device-network/native-apps.ts']),
                lambda: native_apps(root, env, True), lambda: native_apps(root, env))
    # No upstream Chromium snapshot for Linux ARM; do not record absent bytes as installed.
    if platform.system() == 'Linux' and platform.machine().lower() in ('aarch64', 'arm64') and not env.get('AI_PHONE_WHATSAPP_CHROMIUM'):
        print('WhatsApp browser not prepared: this architecture needs a matched bundled runtime')
    else:
        state.phase('messaging-browser', fingerprint(root, ['apps/channels/install-browser.ts', 'apps/channels/whatsapp-browser.ts', 'bun.lock']),
                    lambda: messaging_browser(root, env, True), lambda: messaging_browser(root, env))


def install_guest_images(state, root, env):
    from install_state import fingerprint
    from container_images import image_environment
    images=image_environment(root)
    if images:
        for role in ('workspace','desktop'):
            reference=images['AI_PHONE_'+role.upper()+'_IMAGE']
            state.phase(role+'-image',fingerprint(root,[],{'image':reference}),lambda:None,lambda:image_id(reference))
        return
    proxy = env['AI_PHONE_CONTAINER_PROXY']
    state.phase('workspace-image', fingerprint(root, ['infra/workspace']),
                lambda: run(['docker', 'build', '--build-arg', 'HTTP_PROXY='+proxy, '--build-arg', 'HTTPS_PROXY='+proxy,
                             '-t', 'ai-phone-workspace:20261026', '-f', 'infra/workspace/Dockerfile', 'infra/workspace'], env=env, root=root),
                lambda: image_id('ai-phone-workspace:20261026'))
    state.phase('desktop-image', fingerprint(root, ['infra/desktop', 'compose.desktop.yaml', 'config/clash-apps.json'],
                                           {'workspace_image': image_id('ai-phone-workspace:20261026')}),
                lambda: run(['docker', 'compose', '-f', 'compose.desktop.yaml', 'build', 'desktop'], env=env, root=root),
                lambda: image_id('ai-phone-desktop:20261028'))


def install(root=ROOT, *, observer=None, license_action=None, launch_client=True, minimum_free_gb=35):
    from install_state import InstallState, fingerprint
    if not (root / PROFILE).exists(): configure(root)
    checks = doctor(root, minimum_free_gb=minimum_free_gb)
    for check in checks:
        print(('OK  ' if check['passed'] else 'FAIL ') + check['name'] + (': ' + check['fix'] if check['fix'] else ''))
    downloadable = {'sdk/platform-tools/adb', 'sdk/emulator/emulator'}
    if not all(check['passed'] or check['name'] in downloadable for check in checks):
        raise ValueError('Resolve failed checks and run ./ai-phone install again. No devices were changed.')
    env = environment(root)
    with InstallState(root, observer=observer) as state:
        install_sdk(state, root, env, license_action)
        install_source_components(state, root, env)
        install_guest_images(state, root, env)
        state.phase('services', fingerprint(root, ['scripts/service_start.py', PROFILE]),
                    lambda: start(root=root, launch_client=launch_client), lambda: service_ready('http://127.0.0.1:17860') is None, reuse=False)


def start(root=ROOT, *, launch_client=True):
    if (root/'data/restore-quarantine.json').exists():
        raise ValueError('Restored state is quarantined. Verify recovery and stop the original instance before activation; see docs/public/backup-restore.zh-CN.md')
    from host_platform import host_policy
    host_policy(root)
    from service_start import start_missing
    start_missing(root)
    import time
    deadline=time.monotonic()+45
    while True:
        try:service_ready('http://127.0.0.1:17860');break
        except ValueError:
            if time.monotonic()>=deadline:raise
            time.sleep(1)
    if launch_client:open_client(root)


def service_ready(url):
    request = urllib.request.Request(url + '/api/setup/status', method='POST', data=b'', headers={'Content-Type': 'application/x-protobuf','Origin':url})
    try:
        # Local control traffic must never follow an ambient external HTTP proxy.
        with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=8) as response:
            if response.status != 200 or response.headers.get('content-type') != 'application/x-protobuf': raise ValueError('Setup endpoint is not ready')
    except Exception as e:
        raise ValueError('Local service is not ready; run ./ai-phone doctor and inspect data/ logs') from e


def macos_client(root):
    import plistlib
    candidates=[root/'apps/client/src-tauri/target/release/bundle/macos/AI Phone.app',Path.home()/'Applications/AI Phone.app',Path('/Applications/AI Phone.app')]
    for candidate in candidates:
        try:
            metadata=plistlib.loads((candidate/'Contents/Info.plist').read_bytes())
            if metadata.get('CFBundleIdentifier')=='dev.aiphone.client':return candidate
        except (OSError,ValueError,plistlib.InvalidFileException):continue
    return None


def open_client(root=ROOT):
    url='http://127.0.0.1:17860'
    service_ready(url)
    if platform.system()=='Darwin':
        application=macos_client(root)
        command=['open','-a',application] if application else ['open',url+'/phone']
    elif platform.system()=='Linux':
        application=shutil.which('ai-phone-client')
        command=[application] if application else ['xdg-open',url+'/phone']
    else:
        raise ValueError('Independent client startup is not implemented on this platform')
    run(command,root=root)
    print(url)
    code=root/'data/assistant/setup-code'
    if code.exists() and runtime_env(root).get('AI_PHONE_LOCAL_ENTRY')!='1':
        print('First setup: read the one-time local code from data/assistant/setup-code. It is not included in logs or packages.')


def package(root=ROOT, output=None, public=False):
    from release_package import create_package
    return create_package(root, output or root / 'dist', public=public)


def main():
    parser = argparse.ArgumentParser(description='AI Phone source preview / 本地预览发行工具')
    parser.add_argument('command', choices=['doctor', 'configure', 'install', 'start', 'stop', 'open', 'package', 'env', 'stage', 'backup', 'restore', 'recover-desktop'])
    parser.add_argument('--profile', help='Private JSON host profile; configure only')
    parser.add_argument('--json', action='store_true', help='Machine-readable doctor results')
    parser.add_argument('--output', type=Path, help='Package output folder')
    parser.add_argument('--public', action='store_true', help='Require all publication gates before packaging')
    parser.add_argument('--archive', type=Path, help='Local release archive to verify and stage')
    parser.add_argument('--destination', type=Path, help='Managed installation folder')
    parser.add_argument('--pg-container', default='ai-phone-postgres-1', help='Owned PostgreSQL container; restore requires an empty database')
    parser.add_argument('--desktop-container', default='ai-phone-desktop', help='Owned desktop container for a consistent backup')
    parser.add_argument('--database', help='Empty destination database name; restore only')
    args = parser.parse_args()
    if args.command == 'doctor':
        checks = doctor()
        if args.json: print(json.dumps(checks, ensure_ascii=False, indent=2))
        else:
            for c in checks: print(('OK  ' if c['passed'] else 'FAIL ') + c['name'] + (': ' + c['fix'] if c['fix'] else ''))
        return 0 if all(c['passed'] for c in checks) else 1
    if args.command == 'configure': configure(source=args.profile)
    elif args.command == 'install': install()
    elif args.command == 'start': start()
    elif args.command == 'stop': run(['./scripts/phone-lab-stop.sh'])
    elif args.command == 'open': open_client()
    elif args.command == 'package': print(package(output=args.output, public=args.public))
    elif args.command == 'backup':
        if not args.output:raise ValueError('backup requires --output <private-backup.backup>')
        from state_backup import create_backup
        print(json.dumps(create_backup(ROOT,args.output,pg_container=args.pg_container,desktop_container=args.desktop_container)))
    elif args.command == 'restore':
        if not args.archive or not args.destination:raise ValueError('restore requires --archive and an empty --destination')
        from state_backup import restore_backup
        print(json.dumps(restore_backup(args.archive,args.destination,pg_container=args.pg_container,database=args.database)))
    elif args.command == 'recover-desktop':
        if not args.destination:raise ValueError('recover-desktop requires a quarantined --destination and a new --desktop-container aiphone-recovery-...')
        from state_backup import recover_desktop
        print(json.dumps(recover_desktop(args.destination,args.desktop_container)))
    elif args.command == 'stage':
        if not args.archive or not args.destination: raise ValueError('stage requires --archive and --destination')
        from release_stage import stage_bundle
        print(stage_bundle(args.archive, args.destination))
    elif args.command == 'env':
        for k, v in runtime_env(ROOT).items(): print('export ' + k + '=' + shlex.quote(v))
    return 0


if __name__ == '__main__':
    try: sys.exit(main())
    except (ValueError, OSError, subprocess.SubprocessError, KeyboardInterrupt) as e:
        print('AI Phone: ' + str(e), file=sys.stderr)
        sys.exit(1)
