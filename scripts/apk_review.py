"""Read bounded APK members without extraction and reject private build material."""
from pathlib import Path
import zipfile
from release_package import SECRET


def inspect_apk(path,*,max_entries=256,max_unpacked=32*1024**2):
    with zipfile.ZipFile(path) as apk:
        entries=apk.infolist()
        if len(entries)>max_entries or sum(e.file_size for e in entries)>max_unpacked:
            raise ValueError('APK exceeds its review budget')
        for entry in entries:
            data=apk.read(entry)
            if SECRET.search(data) or SECRET.search(data.replace(b'\0',b'')) or (str(Path.home())+'/').encode() in data:
                raise ValueError('Private material in APK')
