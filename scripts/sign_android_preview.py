"""Sign local Android previews with an isolated non-release identity; never print passwords."""
import json
import os
from pathlib import Path
import secrets
import subprocess
from prepare_android_client import android_environment, ROOT


def sign(source, destination, root=ROOT):
    env = android_environment(root)
    private = root/'data/client-signing'
    private.mkdir(mode=0o700, parents=True, exist_ok=True)
    key = private/'android-preview.jks'
    secret = private/'preview.json'
    if key.exists() != secret.exists(): raise ValueError('Incomplete preview signing identity; preserve and repair it')
    if not secret.exists():
        fd = os.open(secret, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        with os.fdopen(fd, 'w') as f: json.dump({'password': secrets.token_urlsafe(40)}, f)
        env['AI_PHONE_SIGNING_PASSWORD'] = json.loads(secret.read_text())['password']
        subprocess.run([str(Path(env['JAVA_HOME'])/'bin/keytool'), '-genkeypair', '-keystore', str(key),
            '-storepass:env', 'AI_PHONE_SIGNING_PASSWORD', '-keypass:env', 'AI_PHONE_SIGNING_PASSWORD',
            '-alias', 'preview', '-keyalg', 'RSA', '-keysize', '3072', '-validity', '3650',
            '-dname', 'CN=AI Phone local preview,O=AI Phone contributors'], env=env, check=True)
        key.chmod(0o600)
    env['AI_PHONE_SIGNING_PASSWORD'] = json.loads(secret.read_text())['password']
    signer = Path(env['ANDROID_HOME'])/'build-tools/36.0.0'/('apksigner.bat' if os.name == 'nt' else 'apksigner')
    destination.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run([str(signer), 'sign', '--ks', str(key), '--ks-key-alias', 'preview',
        '--ks-pass', 'env:AI_PHONE_SIGNING_PASSWORD', '--out', str(destination), str(source)], env=env, check=True)
    subprocess.run([str(signer), 'verify', '--verbose', str(destination)], env=env, check=True)
    return destination
