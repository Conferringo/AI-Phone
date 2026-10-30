"""Build a local native-client preview; never publish or bundle user/server state."""
import hashlib
import json
from pathlib import Path
import platform
import shutil
import subprocess
import os
import argparse
from native_bundle import inspect_macos_bundle
from client_artifacts import inspect_apk, inspect_deb, inspect_windows, record, notice_manifest

ROOT=Path(__file__).resolve().parent.parent

def build_macos():
    if platform.system()!='Darwin' or platform.machine()!='arm64':
        raise SystemExit('This packaging path is currently validated only on macOS ARM64')
    version=json.loads((ROOT/'config/release-policy.json').read_text())['version']
    environment={**os.environ,'CI':'true','CARGO_BUILD_JOBS':os.environ.get('CARGO_BUILD_JOBS','2')}
    flags=environment.get('CARGO_ENCODED_RUSTFLAGS','').split('\x1f') if environment.get('CARGO_ENCODED_RUSTFLAGS') else []
    flags.append('--remap-path-prefix='+str(Path.home())+'=/build-user')
    environment['CARGO_ENCODED_RUSTFLAGS']='\x1f'.join(flags)
    subprocess.run([ROOT/'scripts/bun','run','--cwd','apps/client','build','--config','src-tauri/gen/host/resources.json','--ci','--no-sign','--','--locked'],cwd=ROOT,env=environment,check=True)
    bundles=ROOT/'apps/client/src-tauri/target/release/bundle'
    inspected=inspect_macos_bundle(ROOT,bundles/'macos/AI Phone.app')
    source=bundles/'dmg'/f'AI Phone_{version}_aarch64.dmg'
    subprocess.run(['hdiutil','verify',str(source)],check=True,stdout=subprocess.DEVNULL)
    destination=ROOT/'dist'/f'AI-Phone-{version}-macos-arm64.dmg'
    destination.parent.mkdir(parents=True,exist_ok=True)
    shutil.copy2(source,destination)
    with destination.open('rb') as source:digest=hashlib.file_digest(source,'sha256').hexdigest()
    destination.with_suffix('.dmg.sha256').write_text(digest+'  '+destination.name+'\n')
    manifest={'version':version,'kind':'native-client-preview','os':'macos','arch':'arm64','sha256':digest,'developer_id_signed':False,'notarized':False,'dependencies_license_review_complete':False,'server_included':False,'remote_upload':False,'server_requirement':'An already configured local/LAN deployment','source_locks':{name:hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in ['bun.lock','apps/client/src-tauri/Cargo.lock']}}
    manifest['source_payload_included']=True
    from prepare_client_host import payload_components
    manifest['prebuilt_components']=payload_components(ROOT)
    manifest['complete_device_bundle']=False
    manifest['local_bootstrap']={'implemented_on':sorted(json.loads((ROOT/'config/host-bootstrap.json').read_text())['hosts']),'fresh_install_acceptance':'pending','user_state_bundled':False}
    manifest['server_requirement']='Client-managed local source deployment, or an existing local/LAN deployment'
    manifest['bundle']=inspected
    manifest['third_party_notices']=notice_manifest(ROOT)
    manifest['source_commit']=os.environ.get('GITHUB_SHA','')
    destination.with_suffix('.dmg.manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(json.dumps({'dmg':str(destination),'application':str(bundles/'macos/AI Phone.app'),'server_included':False,'remote_upload':False}))

def build_other(target, package_only=False):
    version=json.loads((ROOT/'config/release-policy.json').read_text())['version']
    native=ROOT/'apps/client/src-tauri'
    client=ROOT/'apps/client'
    env={**os.environ, 'CI':'true', 'CARGO_BUILD_JOBS':os.environ.get('CARGO_BUILD_JOBS','2'), 'CARGO_INCREMENTAL':'0'}
    cargo_bin=Path(env.get('CARGO_HOME', str(Path.home()/'.cargo')))/'bin'
    if cargo_bin.is_dir():env['PATH']=str(cargo_bin)+os.pathsep+env.get('PATH','')
    env['CARGO_ENCODED_RUSTFLAGS']='--remap-path-prefix='+str(Path.home())+'=/build-user'
    command=['node', str(client/'node_modules/@tauri-apps/cli/tauri.js')]
    dist=ROOT/'dist';dist.mkdir(exist_ok=True)
    if target=='android':
        from prepare_android_client import prepare
        from sign_android_preview import sign
        from android_notices import verify
        env={**prepare(), 'CARGO_ENCODED_RUSTFLAGS':env['CARGO_ENCODED_RUSTFLAGS']}
        if not package_only:
            subprocess.run(command+['android','build','--config','src-tauri/gen/host/resources.json','--apk','--target','aarch64','--ci','--','--locked'],cwd=client,env=env,check=True)
        # Tauri's build creates plugin settings/includes absent after a fresh init.
        # Check the resolved graph before accepting/signing the resulting artifact.
        verify(ROOT,env)
        source=native/'gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk'
        destination=dist/f'AI-Phone-{version}-android-arm64.apk'
        sign(source,destination)
        record(destination,'android','arm64',inspect_apk(destination),signing='local-preview-key; not release identity')
    elif target=='windows':
        cross=platform.system()!='Windows'
        if cross and not package_only:
            cache=Path(env.get('XWIN_CACHE_DIR',str(Path.home()/'.cache/ai-phone-xwin'))).resolve()
            if any(c.isspace() for c in str(cache)):raise SystemExit('XWIN_CACHE_DIR must contain no whitespace')
            if not shutil.which('lld-link',path=env.get('PATH')):raise SystemExit('Add LLVM lld-link to PATH for cargo-xwin')
            if not shutil.which('cargo-xwin',path=env.get('PATH')):raise SystemExit('Install cargo-xwin 0.22.0 before cross-compiling Windows')
            env['XWIN_CACHE_DIR']=str(cache)
        if not package_only:
            subprocess.run(command+['build','--config','src-tauri/gen/host/resources.json',*(['--runner','cargo-xwin'] if cross else []),
                '--target','x86_64-pc-windows-msvc','--bundles','nsis','--ci','--no-sign','--','--locked'],cwd=client,env=env,check=True)
        source=native/f'target/x86_64-pc-windows-msvc/release/bundle/nsis/AI Phone_{version}_x64-setup.exe'
        destination=dist/f'AI-Phone-{version}-windows-x64-setup.exe'
        shutil.copy2(source,destination)
        record(destination,'windows','x64',inspect_windows(destination))
    else:
        if platform.system()!='Linux':raise SystemExit('Build Linux packages on a Linux host/container')
        arch={'aarch64':'arm64','x86_64':'amd64'}.get(platform.machine())
        if not arch:raise SystemExit('Unsupported Linux architecture')
        if not package_only:
            subprocess.run(command+['build','--config','src-tauri/gen/host/resources.json','--bundles','deb','--ci','--no-sign','--','--locked'],cwd=client,env=env,check=True)
        source=native/f'target/release/bundle/deb/AI Phone_{version}_{arch}.deb'
        destination=dist/f'AI-Phone-{version}-linux-{arch}.deb'
        shutil.copy2(source,destination)
        record(destination,'linux',arch,inspect_deb(destination))
    print(json.dumps({'artifact':str(destination),'server_included':False,'remote_upload':False}))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--platform',choices=['macos','windows','linux','android'],default='macos')
    parser.add_argument('--images-runtime',type=Path,help='Include architecture-matched desktop and database image capsule')
    parser.add_argument('--installer-tools',type=Path,help='Include a reviewed portable Python/Node installer runtime')
    parser.add_argument('--android-runtime',type=Path,help='Embed a host-matched Android development runtime; complete distribution review remains required')
    parser.add_argument('--package-only',action='store_true',help='Inspect and package an existing build; does not claim it is current')
    parser.add_argument('--with-web-runtime',action='store_true',help='Include a matching prebuilt standalone web service and pinned Bun; other full-device components remain required')
    parser.add_argument('--with-service-runtime',action='store_true',help='Include matching prebuilt Rust services and OCR on macOS/Linux')
    args=parser.parse_args()
    from check_source import check
    check(ROOT)
    from prepare_client_host import prepare
    if args.with_web_runtime and (args.platform=='android' or args.package_only):parser.error('Build a matching host runtime before including it; Android uses a separate Linux guest payload')
    if args.with_service_runtime and (args.platform not in ('macos','linux') or args.package_only):parser.error('Native service component requires a matching macOS/Linux build')
    if args.images_runtime and (args.platform not in ('macos','linux') or args.package_only):parser.error('Image capsule requires a matching desktop host build')
    if args.installer_tools and (args.platform not in ('macos','linux') or args.package_only):parser.error('Installer tools require a matching supported host build')
    if args.android_runtime and (args.platform not in ('macos','linux') or args.package_only):parser.error('Android runtime embedding requires a matching desktop host build')
    if args.with_web_runtime:
        from bootstrap_bun import bun_path
        bun=bun_path(ROOT)
        build_env={**os.environ,'PATH':str(bun.parent)+os.pathsep+os.environ.get('PATH','')}
        subprocess.run([str(bun),'run','build'],cwd=ROOT,env=build_env,check=True)
    if args.with_service_runtime:
        from build_native_services import build
        build(ROOT)
    prepare(ROOT,with_web_runtime=args.with_web_runtime,with_service_runtime=args.with_service_runtime,android_runtime=args.android_runtime,image_runtime=args.images_runtime,installer_tools=args.installer_tools)
    if args.with_web_runtime:
        from prepare_client_host import verify_prepared_web
        print(json.dumps({'prebuilt_web':verify_prepared_web(ROOT)}))
    if args.with_service_runtime:
        from prepare_client_host import verify_prepared_services
        print(json.dumps({'prebuilt_services':verify_prepared_services(ROOT)}))
    notice_manifest(ROOT)
    if subprocess.check_output(['rustc','--version'],text=True).split()[1]!='1.96.0':
        raise ValueError('Build preview clients with Rust 1.96.0; review runtime notices before changing toolchains')
    if args.platform=='macos':
        if args.package_only:parser.error('--package-only is not supported for macOS')
        build_macos()
    else:build_other(args.platform,args.package_only)
