"""Private host profile. No model keys, shell evaluation or user-data migration."""
import json
import os
import platform
from pathlib import Path


def runtime_env(root):
    root = Path(root)
    profile = root / 'data/deployment/profile.json'
    value = json.loads(profile.read_text()) if profile.exists() else {}
    if not isinstance(value,dict): raise ValueError('Invalid private host profile; expected a JSON object')
    legacy = {}
    if (root / '.env').exists():
        for line in (root / '.env').read_text().splitlines():
            if '=' in line and not line.startswith('#'):
                key, text = line.split('=', 1)
                legacy[key] = text.strip()
    sdk = value.get('sdk_root') or os.environ.get('AI_PHONE_SDK_ROOT') or os.environ.get('ANDROID_HOME')
    if not sdk:
        candidates = [Path.home() / 'Library/Android/sdk', Path('/opt/homebrew/share/android-commandlinetools')]
        sdk = str(next((p for p in candidates if (p / 'platform-tools/adb').exists()), candidates[0]))
    java = value.get('java_home') or os.environ.get('AI_PHONE_JAVA_HOME') or os.environ.get('JAVA_HOME', '')
    if not java and Path('/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home').exists():
        java = '/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home'
    phone = value.get('phone', {})
    desktop = value.get('desktop', {})
    template=phone.get('template')
    x64=template=='android35-x86_64' or (not template and platform.system()=='Linux' and platform.machine()=='x86_64')
    host_proxy = value.get('host_proxy', legacy.get('HOST_HTTP_PROXY', ''))
    container_proxy = value.get('container_proxy', legacy.get('CONTAINER_HTTP_PROXY', ''))
    return {
        'PATH': os.pathsep.join(dict.fromkeys([str(root / '.tools/bin'), '/opt/homebrew/bin', '/usr/local/bin', *os.environ.get('PATH', '/usr/bin:/bin:/usr/sbin:/sbin').split(os.pathsep)])),
        'AI_PHONE_DEFAULT_LOCALE': value.get('locale', ''),
        'AI_PHONE_LOCAL_ENTRY': '1' if value.get('local_entry', legacy.get('AI_PHONE_LOCAL_ENTRY')=='1') else '0',
        'AI_PHONE_DATA_ROOT': str((root / 'data').resolve()),
        'AI_PHONE_PROJECT_ROOT': str(root), 'AI_PHONE_SDK_ROOT': sdk,
        'AI_PHONE_ADB': str(Path(sdk) / 'platform-tools/adb'),
        'AI_PHONE_ANDROID_ABI': 'x86_64' if x64 else 'arm64-v8a',
        'AI_PHONE_CLASH_DESKTOP_DIR': 'linux-amd64' if x64 else 'linux',
        'ANDROID_USER_HOME': str((root/'data/android/user').resolve()),
        'ANDROID_EMULATOR_HOME': str((root/'data/android/user').resolve()),
        'ANDROID_AVD_HOME': str((root/'data/android/avd').resolve()),
        'AI_PHONE_JAVA_HOME': java, 'JAVA_HOME': java,
        'AI_PHONE_HOST_PROXY': host_proxy, 'AI_PHONE_CONTAINER_PROXY': container_proxy,
        'AI_PHONE_EMULATOR_PROXY': value.get('emulator_proxy', '' if profile.exists() else host_proxy),
        'AI_PHONE_PHONE_CPUS': str(phone.get('cpus', 4)),
        'AI_PHONE_PHONE_MEMORY': str(phone.get('memory_mb', 3072)),
        'AI_PHONE_PHONE_STORAGE': str(phone.get('storage_gb', 16)),
        'AI_PHONE_DESKTOP_CPUS': str(desktop.get('cpus', 2)),
        'AI_PHONE_DESKTOP_MEMORY': str(desktop.get('memory_mb', 2560)) + 'm',
    }
