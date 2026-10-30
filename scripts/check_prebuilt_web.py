"""Verify relocated web bytes without a source checkout, package manager or user state."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from prebuilt_web import web_command
from runtime_verification import isolated_environment as runtime_environment

def isolated_environment(root, port):
    environment = runtime_environment(root)
    environment.update(PORT=str(port), HOSTNAME='127.0.0.1', AI_PHONE_LOCAL_ENTRY='1', NEXT_TELEMETRY_DISABLED='1')
    return environment

def ready_page(child, opener, origin):
    deadline = time.monotonic() + 25
    while True:
        if child.poll() is not None:
            raise ValueError('Prebuilt web exited before readiness')
        try:
            with opener.open(origin + '/phone', timeout=2) as response:
                return response.read().decode()
        except OSError:
            if time.monotonic() >= deadline:
                raise ValueError('Prebuilt web readiness timed out')
            time.sleep(0.1)

def verify_asset(opener, origin, url, path):
    with opener.open(origin + url, timeout=3) as response:
        if response.status != 200:
            raise ValueError('Prebuilt asset request failed')
        if hashlib.sha256(response.read()).digest() != hashlib.sha256(path.read_bytes()).digest():
            raise ValueError('Prebuilt asset differs from the installed artifact')

def verify_scripts(root, opener, origin, html):
    assets = re.findall('<script[^>]+src="(/_next/static/[^"?]+)"', html)
    if not assets:
        raise ValueError('Prebuilt page has no runtime assets')
    for name in assets:
        path = root / 'runtime/web/apps/web/.next/static' / name.removeprefix('/_next/static/')
        verify_asset(opener, origin, name, path)
    return len(assets)

def verify_images(root, opener, origin):
    media = root / 'runtime/web/apps/web/.next/static/media'
    image = next(media.glob('*.png'))
    image_url = '/_next/static/media/' + image.name
    verify_asset(opener, origin, image_url, image)
    try:
        opener.open(origin + '/_next/image?url=' + urllib.parse.quote(image_url) + '&w=64&q=75', timeout=3)
        raise ValueError('Unused image optimizer is still enabled')
    except urllib.error.HTTPError as response:
        if response.code != 404:
            raise ValueError('Unused image optimizer is still enabled')

def stop(child):
    if child.poll() is None:
        child.terminate()
    try:
        child.wait(timeout=5)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait()

def verify(root):
    root = Path(root).resolve()
    command = web_command(root)
    with socket.socket() as reservation:
        reservation.bind(('127.0.0.1', 0))
        port = reservation.getsockname()[1]
    environment = isolated_environment(root, port)
    origin = f'http://127.0.0.1:{port}'
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with tempfile.TemporaryFile() as log:
        child = subprocess.Popen(command, cwd=root, env=environment, stdout=log, stderr=log)
        try:
            html = ready_page(child, opener, origin)
            count = verify_scripts(root, opener, origin, html)
            verify_images(root, opener, origin)
            if child.poll() is not None:
                raise ValueError('Prebuilt web exited during verification')
            return {'web_html': 200, 'script_assets_verified': count, 'static_png_verified': True, 'image_optimizer': False, 'runtime': 'bundled Bun', 'package_install_or_build': False, 'private_state_supplied': False}
        except Exception:
            package = root / 'runtime/web/apps/web/node_modules/next'
            print(json.dumps({'next_package_present': (package / 'package.json').is_file(), 'next_entry_present': (package / 'dist/server/next.js').is_file(), 'installation_path_length': len(str(root))}))
            log.seek(0)
            output = log.read().decode(errors='replace')
            if output:
                print(output[-4000:])
            raise
        finally:
            stop(child)
if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    result = verify(args.root)
    if args.output:
        args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))
