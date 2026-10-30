#!/usr/bin/env python3
"""Idempotent second Android provisioning. Independent userdata, token and runtime slot."""
import argparse, json, os, plistlib, secrets, shutil, subprocess, time, uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
SDK=Path(os.environ.get('AI_PHONE_SDK_ROOT','/opt/homebrew/share/android-commandlinetools'))
BASE=ROOT/'data/phones/second'
MANIFEST=ROOT/'data/phones/second.json'
AVDS=ROOT/'data/android/avd'
ENV={**os.environ,'ANDROID_HOME':str(SDK),'ANDROID_USER_HOME':str(ROOT/'data/android/user'),'ANDROID_AVD_HOME':str(AVDS),'ANDROID_ADB_SERVER_PORT':'15037','ADB_SERVER_SOCKET':'tcp:127.0.0.1:15037','JAVA_HOME':os.environ.get('AI_PHONE_JAVA_HOME','/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home'),'AI_PHONE_SERIAL':'emulator-5566'}
ENV['PATH']=ENV['JAVA_HOME']+'/bin:'+str(SDK/'platform-tools')+':'+ENV.get('PATH','')
ADB=[str(SDK/'platform-tools/adb'),'-P','15037','-s','emulator-5566']
def run(args, timeout=90, **kwargs):
 return subprocess.run([str(a) for a in args],env=ENV,check=True,capture_output=True,timeout=timeout,**kwargs)
def adb(*args):return run(ADB+list(args),timeout=40).stdout.decode().strip()
def stage(text):print(text,flush=True)
def job(label,args,env,log):
 config={'Label':label,'ProgramArguments':[str(a) for a in args],'WorkingDirectory':str(ROOT),'RunAtLoad':True,'KeepAlive':True,'ThrottleInterval':5,'EnvironmentVariables':env,'StandardOutPath':str(log),'StandardErrorPath':str(log)}
 source=BASE/(label+'.plist');source.write_bytes(plistlib.dumps(config));source.chmod(0o600)
 domain=f'gui/{os.getuid()}'
 if subprocess.run(['launchctl','print',domain+'/'+label],capture_output=True).returncode==0:
  # Keep live services stable on retry. Config changes require the explicit reconfigure command.
  return
 run(['launchctl','bootstrap',domain,source])
def main():
 p=argparse.ArgumentParser();p.add_argument('--device-id',required=True);p.add_argument('--cpus',type=int,default=2);p.add_argument('--memory-mb',type=int,default=2048);p.add_argument('--storage-gb',type=int,default=16);p.add_argument('--identity',choices=['aiphone','xiaomi8'],default='aiphone');a=p.parse_args();uuid.UUID(a.device_id)
 if a.cpus not in [1,2,4] or a.memory_mb not in [1536,2048,3072,4096] or a.storage_gb not in [16,24,32]:raise SystemExit('Invalid resource profile')
 BASE.mkdir(parents=True,exist_ok=True,mode=0o700)
 if (BASE/'archived').exists():raise SystemExit('Second-phone extension is archived; explicit recovery required')
 conf={'device_id':a.device_id,'slot':2,'template_id':'android35-arm64','cpus':a.cpus,'memory_mb':a.memory_mb,'storage_gb':a.storage_gb,'identity':a.identity}
 if MANIFEST.exists() and json.loads(MANIFEST.read_text())!=conf:raise SystemExit('Existing second phone differs; preserving its disk. Reconfigure explicitly.')
 stage('检查系统镜像与空间')
 if not (SDK/'system-images/android-35/default/arm64-v8a/system.img').exists():raise SystemExit('Android 35 ARM64 image not installed; use SDK manager and accept its licence first')
 if not (AVDS/'ai_phone_second.ini').exists():
  if shutil.disk_usage(ROOT).free<8*1024**3:raise SystemExit('At least 8 GiB free required')
  run([SDK/'cmdline-tools/latest/bin/avdmanager','create','avd','--name','ai_phone_second','--package','system-images;android-35;default;arm64-v8a','--device','pixel_7','--path',AVDS/'phone-second.avd'],input=b'no\n')
  ini=AVDS/'phone-second.avd/config.ini';v=dict(l.split('=',1) for l in ini.read_text().splitlines() if '=' in l);v.update({'hw.ramSize':str(a.memory_mb),'hw.cpu.ncore':str(a.cpus),'hw.lcd.width':'720','hw.lcd.height':'1440','hw.lcd.density':'320','disk.dataPartition.size':f'{a.storage_gb}G','hw.keyboard':'yes','showDeviceFrame':'no','hw.gpu.enabled':'yes','hw.gpu.mode':'host','fastboot.forceColdBoot':'yes','hw.device.name':'phone','AvdId':'ai_phone_second','avd.ini.displayname':'我的手机 2'});ini.write_text(''.join(f'{k}={v}\n' for k,v in v.items()))
 MANIFEST.write_text(json.dumps(conf,indent=2)+'\n');MANIFEST.chmod(0o600)
 stage('启动 Android')
 job('dev.aiphone.emulator.second',[SDK/'emulator/emulator','-avd','ai_phone_second','-port','5566','-no-window','-audio','none','-grpc','8559','-grpc-use-token','-no-boot-anim','-no-snapshot','-writable-system','-feature','-DeviceSkinOverlay','-gpu','host','-memory',a.memory_mb,'-cores',a.cpus,'-dns-server','223.5.5.5,1.1.1.1','-netspeed','full','-netdelay','none','-http-proxy',os.environ.get('AI_PHONE_EMULATOR_PROXY','')],{k:ENV[k] for k in ['ANDROID_HOME','ANDROID_USER_HOME','ANDROID_AVD_HOME','ADB_SERVER_SOCKET','ANDROID_ADB_SERVER_PORT']},BASE/'emulator.log')
 for _ in range(120):
  try:
   if adb('shell','getprop','sys.boot_completed')=='1':break
  except (subprocess.SubprocessError,OSError):pass
  time.sleep(1)
 else:raise SystemExit('Android boot timeout; disk preserved')
 stage('准备输入、桌面与网络')
 run([ROOT/'scripts/android-ready.sh'],timeout=160)
 for kind in ['input','home']:
  apk=ROOT/f'data/android/{kind}/ai-phone-{kind}.apk'
  if not apk.exists():raise SystemExit(f'Missing built-in {kind} APK')
  adb('install','-r',str(apk))
 adb('shell','ime','enable','dev.aiphone.input/.UnicodeIme');adb('shell','ime','set','dev.aiphone.input/.UnicodeIme')
 adb('shell','settings','put','secure','show_ime_with_hard_keyboard','1')
 adb('shell','cmd','notification','allow_listener','dev.aiphone.input/.PhoneNotifications')
 adb('shell','settings','put','secure','enabled_accessibility_services','dev.aiphone.input/.PhoneAttention')
 adb('shell','settings','put','secure','accessibility_enabled','1')
 adb('shell','cmd','package','set-home-activity','dev.aiphone.home/.HomeActivity')
 browser=ROOT/'data/android/downloads/arm64_ChromePublic.apk'
 if browser.exists():adb('install','-r',str(browser))
 run([ROOT/'scripts/android-install-curl.sh'],timeout=90)
 run([ROOT/'scripts/android-install-clash.sh'],timeout=240)
 # Cosmetic identity is explicit; real OS/ABI/build type and security state remain truthful.
 if not (BASE/'identity.done').exists():
  stage('应用型号档案')
  run(['python3',ROOT/'scripts/android-device-profile.py','--apply','--profile',a.identity],timeout=120)
  adb('reboot')
  time.sleep(2);run([ROOT/'scripts/android-ready.sh'],timeout=160)
  (BASE/'identity.done').write_text(a.identity)
 # Remove optional launchers/apps only. System services/providers/settings stay installed.
 run(['python3',ROOT/'scripts/android-cleanup.py'],timeout=160)
 adb('shell','settings','put','system','system_locales','zh-CN')
 adb('shell','settings','put','secure','immersive_mode_confirmations','confirmed')
 adb('shell','settings','put','system','accelerometer_rotation','0');adb('shell','settings','put','system','user_rotation','0')
 adb('shell','settings','put','system','screen_off_timeout','2147483647')
 adb('shell','cmd','input','keyevent','KEYCODE_HOME')
 runner=BASE/'runner';runner.mkdir(exist_ok=True,mode=0o700)
 if not (runner/'token').exists():(runner/'token').write_text(secrets.token_hex(32));(runner/'token').chmod(0o600)
 env={'AI_PHONE_PROJECT_ROOT':str(ROOT),'ANDROID_USER_HOME':ENV['ANDROID_USER_HOME'],'AI_PHONE_DEVICE_ID':a.device_id,'AI_PHONE_SERIAL':'emulator-5566','AI_PHONE_RUNNER_DIR':str(runner),'AI_PHONE_RUNNER_PORT':'17872','AI_PHONE_MEDIA_URL':'http://127.0.0.1:17875','AI_PHONE_MEDIA_PORT':'17875','AI_PHONE_GRPC_PORT':'8559'}
 stage('连接独立画面和控制服务')
 job('dev.aiphone.runner.second',[ROOT/'target/debug/ai-phone-runner'],env,runner/'runner.log')
 job('dev.aiphone.media.second',[ROOT/'.tools/bin/bun',ROOT/'apps/device-stream/server.ts'],env,BASE/'media.log')
 stage('系统已启动')
if __name__=='__main__':main()
