"""Exact capsule gzip/XZ envelopes; domain inventories remain in their owners."""
from contextlib import contextmanager, nullcontext
import io
import lzma
import subprocess
from pathlib import Path
import tarfile
import zlib

MEMORY_LIMIT=128*1024**2
CHUNK=64*1024
MAGIC={'gzip':b'\x1f\x8b','xz':b'\xfd7zXZ\x00'}


class CapsuleReader(io.RawIOBase):
    """Seek by replaying the same FD, with no extra image-sized disk copy."""
    def __init__(self,source,maximum,expected=None):
        super().__init__()
        if type(maximum) is not int or maximum<1:raise ValueError('Invalid capsule budget')
        source.seek(0);magic=source.read(6)
        self.codec=next((name for name,value in MAGIC.items() if magic.startswith(value)),None)
        if self.codec is None or expected is not None and expected!=self.codec:
            raise ValueError('Unsupported capsule compression')
        self.source=source;self.maximum=maximum
        self._restart()

    def _restart(self):
        self.source.seek(0)
        self.decoder=lzma.LZMADecompressor(format=lzma.FORMAT_XZ,memlimit=MEMORY_LIMIT) if self.codec=='xz' else zlib.decompressobj(31)
        self.pending=b'';self.buffer=b'';self.position=0;self.expanded=0;self.ended=False

    def _input(self):
        if self.pending:
            data=self.pending;self.pending=b'';return data
        if self.codec=='xz' and not self.decoder.needs_input:return b''
        data=self.source.read(CHUNK)
        if not data:raise ValueError('Incomplete capsule compression')
        return data

    def _fill(self):
        try:
            output=self.decoder.decompress(self._input(),min(CHUNK,self.maximum-self.expanded+1))
        except (lzma.LZMAError,zlib.error) as error:
            raise ValueError('Invalid capsule compression') from error
        self.expanded+=len(output)
        if self.expanded>self.maximum:raise ValueError('Capsule exceeds raw byte budget')
        if self.codec=='gzip':self.pending=self.decoder.unconsumed_tail
        if self.decoder.eof:
            if self.decoder.unused_data or self.source.read(1):
                raise ValueError('Trailing capsule compression data')
            self.ended=True
        self.buffer=output

    def readable(self):return True

    def seekable(self):return True

    def tell(self):return self.position

    def read(self,size=-1):
        self._checkClosed()
        if size is None:size=-1
        if size==0:return b''
        parts=[];remaining=size
        while remaining!=0:
            if not self.buffer:
                if self.ended:break
                self._fill()
                continue
            count=len(self.buffer) if remaining<0 else min(remaining,len(self.buffer))
            parts.append(self.buffer[:count]);self.buffer=self.buffer[count:]
            self.position+=count
            if remaining>0:remaining-=count
        return b''.join(parts)

    def readinto(self,target):
        data=self.read(len(target));target[:len(data)]=data;return len(data)

    def seek(self,offset,whence=0):
        self._checkClosed()
        if whence not in (0,1):raise ValueError('Unsupported capsule seek')
        target=offset+(self.position if whence==1 else 0)
        if target<0:raise ValueError('Invalid capsule seek')
        if target<self.position:self._restart()
        while self.position<target:
            if not self.read(min(CHUNK,target-self.position)):
                raise ValueError('Incomplete capsule seek')
        return self.position

    def verify(self):
        while self.read(CHUNK):pass


@contextmanager
def decoded(source,maximum,expected=None):
    reader=CapsuleReader(source,maximum,expected)
    try:
        yield reader
        reader.verify()
    finally:
        reader.close()


@contextmanager
def read_tar(source,maximum,*,stream=False,expected=None):
    context=Path(source).open('rb') if isinstance(source,(str,Path)) else nullcontext(source)
    with context as body,decoded(body,maximum,expected) as raw:
        with tarfile.open(fileobj=raw,mode='r|' if stream else 'r:') as archive:
            yield archive


@contextmanager
def write_xz(destination, program, architecture, preset=9):
    from container_images import image_pipe
    destination.flush()
    child=subprocess.Popen([program,'encode-xz',architecture,str(preset),Path(destination.name).parent],stdin=subprocess.PIPE,stdout=destination)
    with image_pipe(child,child.stdin,close_first=True) as source:
        yield source
        source.close()
        if child.wait()!=0:raise ValueError('Native image compression failed')
