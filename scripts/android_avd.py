"""Create a writable Android guest from a read-only SDK image, without Java."""
import os
from pathlib import Path
import tempfile


def checked_resource(value, choices):
    if type(value) is not int or value not in choices:
        raise ValueError('Invalid Android device resources')
    return value


def resources(cpus, memory, storage):
    return {'hw.cpu.ncore':checked_resource(cpus,(1,2,4)),
            'hw.ramSize':checked_resource(memory,(1536,2048,3072,4096)),
            'disk.dataPartition.size':str(checked_resource(storage,(16,24,32)))+'G'}


def existing_device(folder, marker):
    if folder.is_symlink() or marker.is_symlink():
        raise ValueError('Android device path is a link')
    if not (folder.exists() or marker.exists()):return False
    if not (folder/'config.ini').is_file() or not marker.is_file():
        raise ValueError('Existing Android device is incomplete; preserved for recovery')
    return True


def create(sdk, avd_home, abi, cpus, memory, storage):
    values=resources(cpus,memory,storage);sdk=Path(sdk).resolve();home=Path(avd_home)
    if home.is_symlink():
        raise ValueError('Android device parent is a link')
    home=home.resolve()
    image=sdk/f'system-images/android-35/default/{abi}'
    if abi not in ('arm64-v8a','x86_64') or not all((image/name).is_file() for name in ('system.img','vendor.img','kernel-ranchu','ramdisk.img')):
        raise ValueError('Missing matching Android factory image')
    folder=home/'phone-local-01.avd';marker=home/'ai_phone_p2.ini'
    if existing_device(folder,marker):return False
    values.update({'avd.ini.encoding':'UTF-8','abi.type':abi,'hw.cpu.arch':'arm64' if abi=='arm64-v8a' else 'x86_64','image.sysdir.1':str(image)+'/',
                   'tag.id':'default','tag.display':'Default','hw.lcd.width':720,'hw.lcd.height':1440,'hw.lcd.density':320,
                   'hw.keyboard':'yes','hw.gpu.enabled':'yes','hw.gpu.mode':'host','hw.audioInput':'yes','hw.audioOutput':'yes',
                   'hw.gps':'yes','hw.accelerometer':'yes','hw.gyroscope':'yes','hw.battery':'yes','hw.camera.back':'none','hw.camera.front':'none',
                   'showDeviceFrame':'no','PlayStore.enabled':'no','fastboot.forceColdBoot':'yes','fastboot.forceFastBoot':'no'})
    home.mkdir(parents=True,exist_ok=True,mode=0o700)
    with tempfile.TemporaryDirectory(prefix='.phone-',dir=home) as temp:
        work=Path(temp)/'avd';work.mkdir();(work/'config.ini').write_text(''.join(f'{key}={value}\n' for key,value in values.items()))
        record=Path(temp)/'record';record.write_text(f'avd.ini.encoding=UTF-8\npath={folder.resolve()}\ntarget=android-35\n')
        work.rename(folder);record.rename(marker)
    return True


if __name__=='__main__':
    created=create(os.environ['AI_PHONE_SDK_ROOT'],os.environ['ANDROID_AVD_HOME'],os.environ['AI_PHONE_ANDROID_ABI'],int(os.environ['AI_PHONE_PHONE_CPUS']),int(os.environ['AI_PHONE_PHONE_MEMORY']),int(os.environ['AI_PHONE_PHONE_STORAGE']))
    print('Created AI Phone from its factory image.' if created else 'Existing AI Phone preserved.')
