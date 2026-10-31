"""Select stable official SDK archive metadata for maintainer pin updates."""
import argparse
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET

ORIGIN='https://dl.google.com/android/repository/'


def stable_packages(root,component):
    for package in root.findall('remotePackage'):
        if package.get('path')!=component:continue
        channel=package.find('channelRef')
        if channel is not None and channel.get('ref')=='channel-0':yield package


def archive_record(archive):
    complete=archive.find('complete')
    if complete is None:raise ValueError('Missing complete SDK archive')
    name=complete.findtext('url','');checksum=complete.find('checksum');size=complete.findtext('size','')
    if re.fullmatch(r'[A-Za-z0-9_-]+\.zip',name) is None:raise ValueError('Invalid SDK archive filename')
    if not size.isdecimal() or not 0<int(size)<=4*1024**3:raise ValueError('Invalid SDK archive size')
    if checksum is None or checksum.get('type')!='sha1' or re.fullmatch('[0-9a-f]{40}',checksum.text or '') is None:
        raise ValueError('Missing official SDK archive SHA-1')
    return {'url':ORIGIN+name,'size_bytes':int(size),'vendor_sha1':checksum.text}


def package_version(package):
    values=[package.findtext('revision/'+key,'0') for key in ('major','minor','micro')]
    if any(not value.isdecimal() for value in values) or package.find('revision/preview') is not None:
        raise ValueError('Invalid stable SDK revision')
    return '.'.join(values)


def select(data,component,os_name,architecture):
    if len(data)>8*1024**2:raise ValueError('SDK metadata exceeds budget')
    root=ET.fromstring(data);packages=list(stable_packages(root,component))
    if len(packages)!=1:raise ValueError('Expected exactly one stable SDK package')
    package=packages[0];license=package.find('uses-license')
    if license is None or license.get('ref')!='android-sdk-license':raise ValueError('Unexpected stable SDK license')
    archives=[a for a in package.findall('archives/archive') if a.findtext('host-os')==os_name and a.findtext('host-arch',architecture)==architecture]
    if len(archives)!=1:raise ValueError('Expected exactly one matched SDK host archive')
    return {**archive_record(archives[0]),'version':package_version(package),'channel':'stable','license_id':license.get('ref')}


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repository',type=Path,required=True);parser.add_argument('--component',choices=('emulator','platform-tools'),required=True)
    parser.add_argument('--host-os',choices=('linux','windows','macosx'),required=True);parser.add_argument('--host-arch',choices=('x64','aarch64'),required=True)
    args=parser.parse_args();print(json.dumps(select(args.repository.read_bytes(),args.component,args.host_os,args.host_arch),indent=2))
