"""Factory-only sealed WSL verification. Runtime initialization is native Rust."""
import argparse
import json
from pathlib import Path
import platform
import subprocess
import sys
import tempfile

BUNDLE=Path('/opt/aiphone/bundle')
BOOTSTRAP=Path('/usr/local/libexec/aiphone-bootstrap')
INSTALLATION=Path('/var/lib/aiphone/installation')


def staged(home):
    identity=(BUNDLE/'source.sha256').read_text().strip()
    if len(identity)!=64 or any(char not in '0123456789abcdef' for char in identity):
        raise ValueError('Invalid sealed source identity')
    result=subprocess.run([str(BOOTSTRAP),'--stage-source',str(BUNDLE/'source.payload'),str(home),identity],
                          capture_output=True,text=True,check=True)
    root=Path(home).resolve()/'releases'/identity
    if result.stdout!=f'{identity} {root}\n':raise ValueError('Invalid native source staging receipt')
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
        checks={'native':native(root),'web':web(root),'js_integrity':True}
        images=stage(BUNDLE/'images.payload').manifest
        if images['architecture']!='amd64':raise ValueError('WSL images are not amd64')
        checks['image_roles']=sorted(images['images'])
    return {'offline_components':checks,'windows_boot_verified':False}


def main(argv=None):
    sys.path.insert(0,str(Path(__file__).resolve().parent))
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--verify',action='store_true',required=True)
    parser.parse_args(argv)
    print(json.dumps(verify()))


if __name__=='__main__':main()
