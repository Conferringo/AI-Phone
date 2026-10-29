"""Private installation checkpoints. Skip work only when inputs and artifacts match."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import time
from database import atomic_json


def fingerprint(root,paths,values=None):
    root=Path(root);h=hashlib.sha256();h.update(json.dumps(values or {},sort_keys=True).encode())
    for name in sorted(paths):
        path=root/name;files=sorted(path.rglob('*')) if path.is_dir() else [path]
        for file in files:
            if not file.is_file():continue
            if file.is_symlink():raise ValueError('Installation inputs cannot be source symlinks: '+str(file.relative_to(root)))
            h.update(str(file.relative_to(root)).encode()+b'\0')
            with file.open('rb') as stream:
                while chunk:=stream.read(1024*1024):h.update(chunk)
    return h.hexdigest()

class InstallState:
    def __init__(self,root,observer=None):
        self.root=Path(root);self.folder=self.root/'data/deployment';self.path=self.folder/'install-state.json';self.lock=None;self.observer=observer
    def notify(self,name,state):
        if self.observer:self.observer(name,state)
    def __enter__(self):
        self.folder.mkdir(parents=True,exist_ok=True,mode=0o700);self.folder.chmod(0o700)
        fd=os.open(self.folder/'install.lock',os.O_RDWR|os.O_CREAT,0o600);os.fchmod(fd,0o600)
        self.lock=os.fdopen(fd,'a+')
        try:fcntl.flock(self.lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:self.lock.close();self.lock=None;raise ValueError('Another installer is running for this user directory')
        try:
            self.state=json.loads(self.path.read_text()) if self.path.exists() else {'format':1,'phases':{}}
            if self.state.get('format')!=1 or not isinstance(self.state.get('phases'),dict):raise ValueError('Invalid installation checkpoint; existing user data was not changed')
        except BaseException:
            self.lock.close();self.lock=None;raise
        return self
    def __exit__(self,*args):
        if self.lock:self.lock.close();self.lock=None
    def save(self):atomic_json(self.path,self.state)
    def phase(self,name,input_hash,action,verify,*,reuse=True):
        old=self.state['phases'].get(name,{})
        if reuse and old.get('state')=='completed' and old.get('input_sha256')==input_hash:
            try:valid=verify()
            except (ValueError,OSError):valid=False
            if valid and old.get('artifact')==valid:print('Verified existing '+name,flush=True);self.notify(name,'completed');return False
        current={'state':'running','input_sha256':input_hash,'attempt':old.get('attempt',0)+1,'started_at':int(time.time())}
        self.state['phases'][name]=current;self.save();print('Installing '+name,flush=True);self.notify(name,'running')
        try:
            action()
            artifact=verify()
            if not artifact:raise ValueError('Installation artifact verification failed: '+name)
        except BaseException as error:
            current.update(state='interrupted' if isinstance(error,(KeyboardInterrupt,SystemExit)) else 'failed',finished_at=int(time.time()),error='Phase did not complete; correct the failure and rerun install')
            self.save();self.notify(name,current['state']);raise
        current.update(state='completed',finished_at=int(time.time()),artifact=artifact);self.save();self.notify(name,'completed');return True
