"""Regenerate ignored Tauri Android scaffolding and apply versioned project overlays."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tomllib
import json
from runtime_config import runtime_env

ROOT = Path(__file__).resolve().parent.parent


def patch_build_task(path, cli):
    text=path.read_text()
    executable,count=re.subn(r'val executable = .*?;', 'val executable = "node";', text)
    command='val args = listOf('+json.dumps(str(cli)).replace('$', r'\$')+', "android", "android-studio-script");'
    patched,args_count=re.subn(r'val args = listOf\([^\n]*\);', lambda _:command, executable)
    if count!=1 or args_count!=1:
        raise ValueError('Unrecognized Android build task template')
    path.write_text(patched)


def android_environment(root=ROOT):
    config = runtime_env(root)
    sdk = Path(os.environ.get('ANDROID_HOME') or config['AI_PHONE_SDK_ROOT'])
    ndk = Path(os.environ.get('NDK_HOME') or sdk/'ndk/27.0.12077973')
    if not (ndk/'source.properties').is_file():
        raise ValueError('Install Android NDK 27.0.12077973 or set NDK_HOME')
    if not (sdk/'platforms/android-36/android.jar').is_file():
        raise ValueError('Install Android SDK platform 36 before building')
    return {**os.environ, 'ANDROID_HOME': str(sdk), 'NDK_HOME': str(ndk),
            'JAVA_HOME': os.environ.get('JAVA_HOME') or config['JAVA_HOME'],
            'CI': 'true', 'CARGO_BUILD_JOBS': '2', 'CARGO_INCREMENTAL': '0'}


def prepare(root=ROOT):
    env = android_environment(root)
    generated = root/'apps/client/src-tauri/gen/android'
    if not generated.exists():
        subprocess.run(['node', str(root/'apps/client/node_modules/@tauri-apps/cli/tauri.js'),
                        'android', 'init', '--ci', '--skip-targets-install'],
                       cwd=root/'apps/client', env=env, check=True)
    # Use the installed stable API/toolchain; generated files remain disposable.
    patch_build_task(generated/'buildSrc/src/main/java/dev/aiphone/client/kotlin/BuildTask.kt',
                     root/'apps/client/node_modules/@tauri-apps/cli/tauri.js')
    for relative in ['build.gradle.kts', 'buildSrc/build.gradle.kts']:
        path = generated/relative
        text = re.sub(r'com.android.tools.build:gradle:[0-9.]+', 'com.android.tools.build:gradle:8.11.1', path.read_text())
        path.write_text(text)
    wrapper = generated/'gradle/wrapper/gradle-wrapper.properties'
    wrapper.write_text(re.sub(r'gradle-[0-9.]+-bin.zip', 'gradle-8.14.3-bin.zip', wrapper.read_text()))
    app = generated/'app/build.gradle.kts'
    text = re.sub(r'(compileSdk|targetSdk) = \d+', r'\1 = 36', app.read_text())
    text = re.sub(r'minSdk = \d+', 'minSdk = 28', text)
    text = re.sub(r'optimization\s*\{\s*enable = true\s*\}', 'isMinifyEnabled = true', text)
    # The generated AGP 9 template omits Android's standard reflection/JNI rules.
    # AGP 8 needs these explicitly, including enum.values() used by Jackson.
    if 'getDefaultProguardFile(' not in text:
        text = text.replace('proguardFiles(', 'proguardFiles(\n                getDefaultProguardFile("proguard-android-optimize.txt"),')
    app.write_text(text)
    properties = generated/'gradle.properties'
    text = '\n'.join(line for line in properties.read_text().splitlines()
                     if not line.startswith(('android.builtInKotlin=', 'android.newDsl=', 'org.gradle.jvmargs=', 'org.gradle.workers.max=', 'android.useAndroidX=', 'kotlin.compiler.execution.strategy=')))
    properties.write_text(text+'\norg.gradle.jvmargs=-Xmx1536m -Dfile.encoding=UTF-8\norg.gradle.workers.max=2\nandroid.useAndroidX=true\nkotlin.compiler.execution.strategy=in-process\n')
    for source, destination in [('MainActivity.kt', 'java/dev/aiphone/client/MainActivity.kt'),
                                ('AndroidManifest.xml', 'AndroidManifest.xml')]:
        target = generated/'app/src/main'/destination
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(root/'apps/client/android'/source, target)
    shutil.copytree(root/'apps/client/src-tauri/icons/android',
                    generated/'app/src/main/res', dirs_exist_ok=True)
    bridge = generated/'app/src/main/java/dev/aiphone/client/generated/Rust.kt'
    if bridge.exists():
        library = tomllib.loads((root/'apps/client/src-tauri/Cargo.toml').read_text())['lib']['name']
        bridge.write_text(re.sub(r'System.loadLibrary\("[^"]+"\)',
                                'System.loadLibrary("'+library+'")', bridge.read_text()))
    # Retire old library symlinks if the Rust library name changed; never ship both.
    lib_name = tomllib.loads((root/'apps/client/src-tauri/Cargo.toml').read_text())['lib']['name']
    for old in (generated/'app/src/main/jniLibs').glob('*/libai_phone_client.so'):
        if lib_name != 'ai_phone_client' and old.is_symlink():old.unlink()
    return env


if __name__ == '__main__':
    prepare()
    print('Android source overlays prepared; no device data changed')
