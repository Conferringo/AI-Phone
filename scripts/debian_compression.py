"""Maintainer-only DEB repacking; the installed TAR streams stay byte-identical."""
import gzip
import hashlib
import lzma
import os
from pathlib import Path
import shutil
import tempfile

MAGIC=b'!<arch>\n'
MEMBERS=('debian-binary','control.tar.gz','data.tar.gz')
CHUNK=64*1024


class Member:
    def __init__(self,source,size):
        self.source=source;self.remaining=size

    def read(self,size):
        count=min(size,self.remaining)
        value=self.source.read(count)
        if len(value)!=count:raise ValueError('Truncated DEB member')
        self.remaining-=count
        return value


def member_header(source,expected):
    header=source.read(60)
    if len(header)!=60 or header[58:]!=b'`\n' or header[:16].rstrip() not in (expected.encode(),(expected+'/').encode()):
        raise ValueError('Unexpected DEB member header')
    size=header[48:58].strip()
    if not size.isdigit():raise ValueError('Invalid DEB member size')
    return header,int(size)


def transcode(source,target):
    identity=hashlib.sha256();size=0
    with gzip.GzipFile(fileobj=source,mode='rb') as incoming,lzma.LZMAFile(target,mode='wb',preset=6) as outgoing:
        while data:=incoming.read(CHUNK):
            identity.update(data);size+=len(data);outgoing.write(data)
    return {'raw_bytes':size,'raw_sha256':identity.hexdigest()}


def write_member(source,target,expected):
    header,size=member_header(source,expected);part=Member(source,size)
    name=expected.removesuffix('.gz')+'.xz' if expected.endswith('.gz') else expected
    offset=target.tell();target.write(header);start=target.tell()
    if expected=='debian-binary':
        if size!=4 or part.read(4)!=b'2.0\n':raise ValueError('Unsupported DEB version')
        target.write(b'2.0\n');receipt=None
    else:receipt=transcode(part,target)
    if part.remaining or source.read(size%2)!=b'\n'[:size%2]:raise ValueError('Invalid DEB member boundary')
    end=target.tell();length=end-start
    if length>9999999999:raise ValueError('DEB member exceeds ar format')
    changed=(name+'/').encode().ljust(16)+header[16:48]+str(length).encode().ljust(10)+header[58:]
    target.seek(offset);target.write(changed);target.seek(end)
    if length%2:target.write(b'\n')
    return name,receipt


def recompress(source,destination):
    """Publish the smaller valid representation, preserving the previous output on failure."""
    source,destination=Path(source),Path(destination)
    destination.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=destination.parent,prefix='.deb-',delete=False) as output:
        temporary=Path(output.name)
        try:
            with source.open('rb') as original:
                if original.read(8)!=MAGIC:raise ValueError('Invalid DEB archive')
                output.write(MAGIC);receipts=dict(write_member(original,output,name) for name in MEMBERS)
                if original.read(1):raise ValueError('Unexpected trailing DEB data')
                codec='xz'
                if output.tell()>=source.stat().st_size:
                    original.seek(0);output.seek(0);output.truncate();shutil.copyfileobj(original,output,CHUNK);codec='gzip'
            output.flush();os.fsync(output.fileno());size=output.tell();output.close()
            os.chmod(temporary,source.stat().st_mode&0o777);os.replace(temporary,destination)
            if codec=='gzip':receipts={name.removesuffix('.xz')+'.gz' if name.endswith('.xz') else name:value for name,value in receipts.items()}
            return {'compression':codec,'bytes':size,'members':{name:value for name,value in receipts.items() if value is not None}}
        finally:temporary.unlink(missing_ok=True)
