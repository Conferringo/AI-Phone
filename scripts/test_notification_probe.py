"""Execute the real probe with a JVM-only Binder fixture, never an Android device."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent


def javac_path():
    candidates = [Path(os.environ.get('JAVA_HOME','/nonexistent'))/'bin/javac',
        Path('/opt/homebrew/opt/openjdk@17/bin/javac')]
    native = shutil.which('javac')
    if native and not (sys.platform == 'darwin' and native == '/usr/bin/javac'):
        candidates.append(Path(native))
    return next((path.resolve() for path in candidates if path.is_file()), None)


@unittest.skipUnless(javac_path(), 'JDK compiler required for the isolated Binder fixture')
class NotificationProbeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix='probe-java-',dir=HERE.parent)
        cls.addClassCleanup(cls.temp.cleanup)
        cls.root = Path(cls.temp.name)
        fixtures = {
            'android/content/ComponentName.java': '''package android.content;
public final class ComponentName {
 public final String pkg, name;
 public ComponentName(String pkg,String name) { this.pkg=pkg;this.name=name; }
}''',
            'android/os/IBinder.java': 'package android.os; public interface IBinder {}',
            'android/os/Process.java': '''package android.os;
public final class Process { public static int myUid() { return Integer.getInteger("fixture.uid",0); } }''',
            'android/os/ServiceManager.java': '''package android.os;
public final class ServiceManager {
 public static IBinder getService(String name) {
  if (!"notification".equals(name)) throw new IllegalArgumentException();
  return Boolean.getBoolean("fixture.missing") ? null : new IBinder() {};
 }
}''',
            'android/app/INotificationManager.java': '''package android.app;
import android.content.ComponentName; import android.os.IBinder;
public final class INotificationManager {
 public boolean isNotificationListenerAccessGrantedForUser(ComponentName component,int user) {
  if (!"dev.aiphone.input".equals(component.pkg)
    || !"dev.aiphone.input.PhoneNotifications".equals(component.name) || user!=0)
   throw new IllegalArgumentException();
  if (Boolean.getBoolean("fixture.error")) throw new IllegalStateException("fixture-user-notification-content");
  return Boolean.getBoolean("fixture.granted");
 }
 public static final class Stub { public static INotificationManager asInterface(IBinder binder) { return new INotificationManager(); } }
}''',
        }
        sources = []
        for name,text in fixtures.items():
            path=cls.root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(text);sources.append(str(path))
        cls.classes=cls.root/'classes';cls.classes.mkdir()
        compiler=javac_path();cls.java=compiler.parent/'java'
        subprocess.run([str(compiler),'--release','8','-d',str(cls.classes),*sources,
            str(HERE/'android-input-probe/NotificationAccess.java')],capture_output=True,text=True,check=True,timeout=20)

    def run_probe(self, properties=(), args=()):
        result=subprocess.run([str(self.java),*properties,'-cp',str(self.classes),
            'dev.aiphone.input.probe.NotificationAccess',*args],capture_output=True,text=True,timeout=5)
        self.assertNotIn('fixture-user-notification-content',result.stdout+result.stderr)
        return result

    def test_authority_true_and_false_return_only_boolean(self):
        for granted in ('true','false'):
            with self.subTest(granted=granted):
                result=self.run_probe(['-Dfixture.granted='+granted])
                self.assertEqual(result.returncode,0,result.stderr)
                self.assertEqual(result.stdout,granted+'\n');self.assertEqual(result.stderr,'')

    def test_unknown_binder_results_never_become_false(self):
        for prop in ('-Dfixture.error=true','-Dfixture.missing=true'):
            with self.subTest(prop=prop):
                result=self.run_probe([prop])
                self.assertNotEqual(result.returncode,0);self.assertEqual(result.stdout,'')
                self.assertEqual(result.stderr,'Android input notification observation is unknown\n')

    def test_non_root_or_arbitrary_component_arguments_are_rejected(self):
        for properties,args in [(['-Dfixture.uid=2000'],[]),([],['other/.Listener'])]:
            with self.subTest(properties=properties,args=args):
                result=self.run_probe(properties,args)
                self.assertNotEqual(result.returncode,0);self.assertEqual(result.stdout,'')
                self.assertEqual(result.stderr,'Android input notification observation is unknown\n')


class SourceProbeBuildTests(unittest.TestCase):
    def test_source_cli_compiles_the_shared_probe_without_installing_an_apk(self):
        compiler=javac_path()
        sdk=Path(os.environ.get('ANDROID_HOME','/opt/homebrew/share/android-commandlinetools'))
        if not compiler or not (sdk/'platforms/android-35/android.jar').is_file() or not (sdk/'build-tools/35.0.0/d8').is_file():
            self.skipTest('Installed Android 35 SDK and JDK required for the source-only compile fixture')
        with tempfile.TemporaryDirectory(prefix='probe-sdk-',dir=HERE.parent) as directory:
            root=Path(directory);source=root/'scripts/android-input-probe/NotificationAccess.java'
            source.parent.mkdir(parents=True);shutil.copyfile(HERE/'android-input-probe/NotificationAccess.java',source)
            env={**os.environ,'ANDROID_HOME':str(sdk),'JAVA_HOME':str(compiler.parent.parent),
                'PATH':'/usr/bin:/bin','PYTHONDONTWRITEBYTECODE':'1'}
            argv=[sys.executable,str(HERE/'prebuilt_phone.py'),'--root',str(root),'--notification-probe']
            for _ in range(2):
                result=subprocess.run(argv,env=env,capture_output=True,text=True,timeout=30)
                self.assertEqual(result.returncode,0,result.stderr)
                path=root/'data/android/notification-probe/notification-access.jar'
                self.assertEqual(result.stdout.strip(),str(path));self.assertLess(path.stat().st_size,64*1024)
                from prebuilt_phone import inspect_probe
                inspect_probe(path)
            self.assertEqual(list(root.rglob('*.apk')),[])
            self.assertFalse((root/'data/android/input-setup.pending.json').exists())


if __name__ == '__main__':unittest.main()
