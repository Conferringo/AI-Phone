"""Build a local native-client preview; never publish or bundle user/server state."""
import hashlib
import json
from pathlib import Path
import platform
import shutil
import subprocess
import os
import argparse
import tempfile
from native_bundle import inspect_macos_bundle
from client_artifacts import inspect_apk, inspect_deb, inspect_appimage, inspect_windows, record, notice_manifest

ROOT=Path(__file__).resolve().parent.parent
WINDOWS_TARGET='x86_64-pc-windows-msvc'

def native_environment():
    env={**os.environ, 'CI':'true', 'CARGO_BUILD_JOBS':os.environ.get('CARGO_BUILD_JOBS','2'), 'CARGO_INCREMENTAL':'0'}
    env['CARGO_HOME']=env.get('CARGO_HOME', str(Path.home()/'.cargo'))
    cargo_bin=Path(env['CARGO_HOME'])/'bin'
    if cargo_bin.is_dir():env['PATH']=str(cargo_bin)+os.pathsep+env.get('PATH','')
    env['CARGO_ENCODED_RUSTFLAGS']='--remap-path-prefix='+str(Path.home())+'=/build-user'
    return env

def windows_cargo(operation):
    subprocess.run(['cargo',operation,'--manifest-path',str(ROOT/'apps/client/src-tauri/Cargo.toml'),
                    '--release','--target',WINDOWS_TARGET,'--features','tauri/custom-protocol',
                    *(['--bins'] if operation=='build' else []),'--locked'],
                   cwd=ROOT,env=native_environment(),check=True)

def build_macos():
    if platform.system()!='Darwin' or platform.machine()!='arm64':
        raise SystemExit('This packaging path is currently validated only on macOS ARM64')
    version=json.loads((ROOT/'config/release-policy.json').read_text())['version']
    environment={**os.environ,'CI':'true','CARGO_BUILD_JOBS':os.environ.get('CARGO_BUILD_JOBS','2')}
    flags=environment.get('CARGO_ENCODED_RUSTFLAGS','').split('\x1f') if environment.get('CARGO_ENCODED_RUSTFLAGS') else []
    flags.append('--remap-path-prefix='+str(Path.home())+'=/build-user')
    environment['CARGO_ENCODED_RUSTFLAGS']='\x1f'.join(flags)
    subprocess.run([ROOT/'scripts/bun','run','--cwd','apps/client','build','--config','src-tauri/gen/host/resources.json','--bundles','app','--ci','--no-sign','--','--locked'],cwd=ROOT,env=environment,check=True)
    bundles=ROOT/'apps/client/src-tauri/target/release/bundle'
    inspected=inspect_macos_bundle(ROOT,bundles/'macos/AI Phone.app')
    destination=ROOT/'dist'/f'AI-Phone-{version}-macos-arm64.dmg'
    from macos_disk_image import build as build_disk_image
    build_disk_image(bundles/'macos/AI Phone.app',destination)
    with destination.open('rb') as source:digest=hashlib.file_digest(source,'sha256').hexdigest()
    destination.with_suffix('.dmg.sha256').write_text(digest+'  '+destination.name+'\n')
    manifest={'version':version,'kind':'native-client-preview','os':'macos','arch':'arm64','sha256':digest,'developer_id_signed':False,'notarized':False,'dependencies_license_review_complete':False,'remote_upload':False,'source_locks':{name:hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in ['bun.lock','apps/client/src-tauri/Cargo.lock']}}
    manifest['source_payload_included']=True
    from prepare_client_host import payload_components
    from client_delivery import delivery_metadata,installer_metadata
    manifest.update(delivery_metadata(payload_components(ROOT),'macos-arm64'))
    manifest['installer_delivery']=installer_metadata(destination,'macos-arm64')
    manifest['local_bootstrap']={'implemented_on':sorted(json.loads((ROOT/'config/host-bootstrap.json').read_text())['hosts']),'fresh_install_acceptance':'pending','user_state_bundled':False}
    manifest['bundle']=inspected
    manifest['third_party_notices']=notice_manifest(ROOT)
    manifest['source_commit']=os.environ.get('GITHUB_SHA','')
    destination.with_suffix('.dmg.manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(json.dumps({'dmg':str(destination),'application':str(bundles/'macos/AI Phone.app'),'server_included':manifest['server_included'],'complete_device_bundle':manifest['complete_device_bundle'],'remote_upload':False}))

def compact_appimage(root,appdir,source,arch,env):
    # Reuse Tauri's own tool and the built image's runtime; change compression only.
    plugin=root/'apps/client/src-tauri/target/.tauri/linuxdeploy-plugin-appimage.AppImage'
    with tempfile.TemporaryDirectory(prefix='ai-phone-appimage-pack-') as temporary:
        work=Path(temporary)
        subprocess.run([plugin,'--appimage-extract'],cwd=work,check=True,stdout=subprocess.DEVNULL)
        tools=work/'squashfs-root/usr'
        offset=int(subprocess.check_output([source,'--appimage-offset'],text=True))
        runtime=work/'runtime'
        with source.open('rb') as original:runtime.write_bytes(original.read(offset))
        packed=work/'packed.AppImage'
        environment={**env,'ARCH':{'amd64':'x86_64','arm64':'aarch64'}[arch]}
        subprocess.run([tools/'bin/appimagetool',appdir,packed,'--runtime-file',runtime,'--comp','zstd',
                        '--mksquashfs-opt','-b','--mksquashfs-opt','1M',
                        '--mksquashfs-opt','-Xcompression-level','--mksquashfs-opt','19'],
                       env=environment,check=True)
        shutil.copy2(packed,source)


def build_linux(root,version,env,command,package_only):
    if platform.system()!='Linux':raise SystemExit('Build Linux packages on a Linux host/container')
    arch={'aarch64':'arm64','x86_64':'amd64'}.get(platform.machine())
    if not arch:raise SystemExit('Unsupported Linux architecture')
    if not package_only:
        patchelf=shutil.which('patchelf',path=env.get('PATH'))
        if not patchelf:raise ValueError('Install native patchelf before building Linux AppImages')
        env={**env,'PATCHELF':str(root/'scripts/linux-appimage-patchelf.sh'),'AI_PHONE_REAL_PATCHELF':patchelf,
             'NO_STRIP':'1','LDAI_COMP':'zstd'}
        local_tools=json.dumps({'bundle':{'useLocalToolsDir':True}},separators=(',',':'))
        subprocess.run(command+['build','--verbose','--config','src-tauri/gen/host/resources.json','--config',local_tools,'--bundles','deb,appimage','--ci','--no-sign','--','--locked'],cwd=root/'apps/client',env=env,check=True)
        folder=root/'apps/client/src-tauri/target/release/bundle/appimage'
        label='aarch64' if arch=='arm64' else arch
        compact_appimage(root,folder/'AI Phone.AppDir',folder/f'AI Phone_{version}_{label}.AppImage',arch,env)
    variants=[('deb','deb',arch),('appimage','AppImage','aarch64' if arch=='arm64' else arch)]
    results=[]
    for kind,extension,label in variants:
        source=root/f'apps/client/src-tauri/target/release/bundle/{kind}/AI Phone_{version}_{label}.{extension}'
        destination=root/'dist'/f'AI-Phone-{version}-linux-{arch}.{extension}'
        shutil.copy2(source,destination)
        inspection=inspect_deb(destination,root) if kind=='deb' else inspect_appimage(destination,arch,root)
        manifest=record(destination,'linux',arch,inspection,root=root)
        results.append({'artifact':str(destination),'server_included':manifest['server_included'],
                        'complete_device_bundle':manifest['complete_device_bundle']})
    return results


def build_other(target, package_only=False):
    version=json.loads((ROOT/'config/release-policy.json').read_text())['version']
    native=ROOT/'apps/client/src-tauri'
    client=ROOT/'apps/client'
    env=native_environment()
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
        manifest=record(destination,'android','arm64',inspect_apk(destination),signing='local-preview-key; not release identity')
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
                '--target',WINDOWS_TARGET,'--bundles','nsis','--ci','--no-sign','--','--locked'],cwd=client,env=env,check=True)
        source=native/f'target/{WINDOWS_TARGET}/release/bundle/nsis/AI Phone_{version}_x64-setup.exe'
        destination=dist/f'AI-Phone-{version}-windows-x64-setup.exe'
        shutil.copy2(source,destination)
        manifest=record(destination,'windows','x64',inspect_windows(destination))
    else:
        results=build_linux(ROOT,version,env,command,package_only)
        print(json.dumps({'artifacts':results,'remote_upload':False}))
        return
    print(json.dumps({'artifact':str(destination),'server_included':manifest['server_included'],'complete_device_bundle':manifest['complete_device_bundle'],'remote_upload':False}))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--platform',choices=['macos','windows','linux','android'],default='macos')
    parser.add_argument('--images-runtime',type=Path,help='Include architecture-matched desktop and database image capsule')
    parser.add_argument('--installer-tools',type=Path,help='Include a reviewed portable Python/Node installer runtime')
    parser.add_argument('--android-runtime',type=Path,help='Embed a host-matched Android development runtime; complete distribution review remains required')
    parser.add_argument('--package-only',action='store_true',help='Inspect and package an existing build; does not claim it is current')
    parser.add_argument('--with-web-runtime',action='store_true',help='Include a matching prebuilt standalone web service and pinned Bun; other full-device components remain required')
    parser.add_argument('--with-service-runtime',action='store_true',help='Include matching prebuilt Rust services and OCR on macOS/Linux')
    parser.add_argument('--with-phone-apps',action='store_true',help='Include verified prebuilt phone input and launcher applications')
    parser.add_argument('--with-container-host',action='store_true',help='Include a matched Lima/ISO/offline engine component')
    parser.add_argument('--with-js-services',action='store_true',help='Include prebuilt media/channel services and their production dependencies')
    parser.add_argument('--with-guest-apps',action='store_true',help='Include the pinned Android Clash application matching the guest ABI')
    parser.add_argument('--wsl-runtime',type=Path,help='Verified WSL backend capsule for the Windows native installer')
    parser.add_argument('--linux-ocr',type=Path,help='Verified offline Linux x64 OCR component')
    args=parser.parse_args()
    from check_source import check
    check(ROOT)
    from prepare_client_host import prepare
    if args.wsl_runtime and (args.platform!='windows' or args.package_only):parser.error('WSL runtime requires a Windows native build')
    if args.with_web_runtime and (args.platform=='android' or args.package_only):parser.error('Build a matching host runtime before including it; Android uses a separate Linux guest payload')
    if args.with_service_runtime and (args.platform not in ('macos','linux') or args.package_only):parser.error('Native service component requires a matching macOS/Linux build')
    if args.linux_ocr and (args.platform!='linux' or not args.with_service_runtime):parser.error('Offline Linux OCR requires a matching Linux native service build')
    if args.with_phone_apps and args.package_only:parser.error('Phone applications must be supplied before compilation')
    if args.with_guest_apps and args.package_only:parser.error('Guest applications must be supplied before compilation')
    if args.with_container_host and (args.platform not in ('macos','linux') or args.package_only):parser.error('Container-host components require a matching supported desktop host')
    if args.with_js_services and (not args.with_web_runtime or args.package_only):parser.error('JS services require the matching prebuilt web/Bun runtime')
    if args.images_runtime and (args.platform not in ('macos','linux') or args.package_only):parser.error('Image capsule requires a matching desktop host build')
    if args.installer_tools and (args.platform not in ('macos','linux') or args.package_only):parser.error('Installer tools require a matching supported host build')
    if args.android_runtime and (args.platform not in ('macos','linux','windows') or args.package_only):parser.error('Android runtime embedding requires a matching desktop host build')
    if args.with_web_runtime:
        from bootstrap_bun import bun_path
        bun=bun_path(ROOT)
        build_env={**os.environ,'PATH':str(bun.parent)+os.pathsep+os.environ.get('PATH','')}
        subprocess.run([str(bun),'run','build'],cwd=ROOT,env=build_env,check=True)
    if args.with_service_runtime:
        from build_native_services import build
        build(ROOT)
    if args.with_js_services:
        from prebuilt_js import build as build_js
        build_js(ROOT)
    prepare(ROOT,with_web_runtime=args.with_web_runtime,with_service_runtime=args.with_service_runtime,with_phone_apps=args.with_phone_apps,with_container_host=args.with_container_host,with_js_services=args.with_js_services,with_guest_apps=args.with_guest_apps,android_runtime=args.android_runtime,image_runtime=args.images_runtime,installer_tools=args.installer_tools,linux_ocr=args.linux_ocr,wsl_runtime=args.wsl_runtime)
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
