"""Remove only optional apps for user 0; preserve framework/providers and rollback data."""
from pathlib import Path
import subprocess,json,os
root=Path(__file__).resolve().parent.parent
adb=[str(root/'scripts/android-adb')]
def run(*args):return subprocess.run(adb+list(args),capture_output=True,text=True,timeout=30)
# Refuse to remove the old validation IME before its independent replacement is ready.
assert 'dev.aiphone.input/.UnicodeIme' in run('shell','settings','get','secure','default_input_method').stdout
assert 'org.cromite.cromite' in run('shell','pm','list','packages','org.cromite.cromite').stdout
packages=['dev.aiphone.bridge','com.android.camera2','com.android.calendar','com.android.deskclock',
 'com.android.gallery3d','com.android.music','com.android.musicfx','com.android.contacts',
 'com.android.dialer','com.android.messaging','com.android.quicksearchbox','com.android.stk',
 'org.chromium.webview_shell','com.android.devicediagnostics','com.android.traceur',
 'com.android.egg','com.android.dreams.basic','com.android.dreams.phototable','com.android.emulator.multidisplay']
results=[]
for package in packages:
 if not run('shell','pm','list','packages',package).stdout.strip():
  results.append({'package':package,'status':'already absent'});continue
 r=run('shell','pm','uninstall','-k','--user','0',package)
 if 'Success' not in r.stdout:raise SystemExit(package+': '+r.stdout+r.stderr)
 results.append({'package':package,'status':'removed for user 0; data preserved'})
# Keep both system navigation and Settings; framework providers stay installed.
run('shell','cmd','role','add-role-holder','--user','0','android.app.role.BROWSER','org.cromite.cromite')
report={'removed':results,'launcher':run('shell','cmd','package','query-activities','--brief','-a','android.intent.action.MAIN','-c','android.intent.category.LAUNCHER').stdout}
(root/('data/phones/second/cleanup.json' if os.environ.get('AI_PHONE_SERIAL')=='emulator-5566' else 'docs/evidence/p2/android-cleanup.json')).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print(json.dumps(report,ensure_ascii=False,indent=2))
