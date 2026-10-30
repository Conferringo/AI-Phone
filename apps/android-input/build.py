"""Build the service-only Unicode IME using the installed SDK, without Gradle downloads."""
from pathlib import Path
import os, subprocess, zipfile

source = Path(__file__).resolve().parent
root = source.parents[1]
out = root / 'data/android/input'
sdk = Path(os.environ['ANDROID_HOME'])
jdk = Path(os.environ['JAVA_HOME']) / 'bin'
tools = sdk / 'build-tools/35.0.0'
jar = sdk / 'platforms/android-35/android.jar'
out.mkdir(parents=True, exist_ok=True)
(out/'gen').mkdir(exist_ok=True)
(out/'classes').mkdir(exist_ok=True)
(out/'dex').mkdir(exist_ok=True)
def run(*args): subprocess.run([str(arg) for arg in args], check=True)
run(tools/'aapt2', 'compile', '--dir', source/'res', '-o', out/'resources.zip')
run(tools/'aapt2', 'link', '-o', out/'base.apk', '-I', jar, '--manifest', source/'AndroidManifest.xml', '--java', out/'gen', '--min-sdk-version', '26', '--target-sdk-version', '35', out/'resources.zip')
java = list((source/'src').rglob('*.java')) + list((out/'gen').rglob('*.java'))
run(jdk/'javac', '--release', '8', '-classpath', jar, '-d', out/'classes', *java)
classes = list((out/'classes').rglob('*.class'))
run(tools/'d8', '--lib', jar, '--min-api', '26', '--output', out/'dex', *classes)
with zipfile.ZipFile(out/'base.apk', 'a') as apk:
    apk.write(out/'dex/classes.dex', 'classes.dex')
run(tools/'zipalign', '-f', '4', out/'base.apk', out/'aligned.apk')
if not (out/'debug.jks').exists():
    run(jdk/'keytool', '-genkeypair', '-keystore', out/'debug.jks', '-storepass', 'android', '-keypass', 'android', '-alias', 'androiddebugkey', '-keyalg', 'RSA', '-keysize', '2048', '-validity', '3650', '-dname', 'CN=AI Phone Input,O=Local Development,C=CN')
run(tools/'apksigner', 'sign', '--ks', out/'debug.jks', '--ks-pass', 'pass:android', '--out', out/'ai-phone-input.apk', out/'aligned.apk')
run(tools/'apksigner', 'verify', out/'ai-phone-input.apk')
print('Built and verified:', out/'ai-phone-input.apk')
