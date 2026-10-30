"""App-owned Linux supervisors. A receipt never authorizes killing a reused PID."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import threading
import time
from database import atomic_json


def folder_for(root, name):
    if not re.fullmatch(r'[a-z][a-z0-9-]{0,39}',name):raise ValueError('Invalid service name')
    folder=Path(root).resolve()/'data/services';folder.mkdir(parents=True,exist_ok=True,mode=0o700)
    return folder


def process_start(pid):
    try:
        path=Path('/proc')/str(pid)
        if path.stat().st_uid!=os.getuid():return None
        fields=(path/'stat').read_text().rsplit(')',1)[1].split()
        return None if fields[0]=='Z' else fields[19]
    except (OSError,ValueError,IndexError):return None


def unit_state(root, name):
    try:
        value=json.loads((folder_for(root,name)/(name+'.json')).read_text())
        if value.get('root')!=str(Path(root).resolve()) or type(value.get('pid')) is not int:return None
        if process_start(value['pid'])!=value.get('start_ticks'):return None
        args=(Path('/proc')/str(value['pid'])/'cmdline').read_bytes().split(b'\0')
        if str(Path(__file__).resolve()).encode() not in args or name.encode() not in args:return None
        return value
    except (OSError,ValueError):return None


def start_unit(root, name, command, environment):
    root=Path(root).resolve();folder=folder_for(root,name)
    spec={'command':list(map(str,command)),'environment':environment}
    digest=hashlib.sha256(json.dumps(spec,sort_keys=True).encode()).hexdigest()
    fd=os.open(folder/(name+'.lock'),os.O_RDWR|os.O_CREAT,0o600)
    try:
        try:fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:
            deadline=time.monotonic()+5
            while not (old:=unit_state(root,name)) and time.monotonic()<deadline:time.sleep(.05)
            if not old:raise ValueError('Service ownership is unknown; no process was changed')
            if old.get('spec_sha256')!=digest:raise ValueError('A running task has a different service configuration; stop the owned installation before upgrading')
            return False
        atomic_json(folder/(name+'.spec.json'),spec)
        log=os.open(folder/(name+'.log'),os.O_WRONLY|os.O_CREAT|os.O_APPEND,0o600)
        try:
            child=subprocess.Popen([sys.executable,str(Path(__file__).resolve()),'supervise',str(root),name,str(fd),digest],
                                   cwd=root,stdin=subprocess.DEVNULL,stdout=log,stderr=log,pass_fds=(fd,),start_new_session=True)
            # Reap supervisors started by this process without blocking installer exit.
            threading.Thread(target=child.wait,daemon=True).start()
        finally:os.close(log)
        deadline=time.monotonic()+5
        while time.monotonic()<deadline:
            if unit_state(root,name):return True
            time.sleep(.05)
        raise ValueError('Owned service supervisor did not start; inspect its private log')
    finally:os.close(fd)


def stop_unit(root, name):
    value=unit_state(root,name)
    if not value:return False
    os.kill(value['pid'],signal.SIGTERM)
    deadline=time.monotonic()+8
    while unit_state(root,name) and time.monotonic()<deadline:time.sleep(.05)
    if unit_state(root,name):raise ValueError('Owned service did not stop; no PID was force-killed')
    return True


def supervise(root, name, fd, digest):
    folder=folder_for(root,name);spec=json.loads((folder/(name+'.spec.json')).read_text())
    if hashlib.sha256(json.dumps(spec,sort_keys=True).encode()).hexdigest()!=digest:raise ValueError('Service configuration changed during startup')
    stopping=False
    def stop(*_):
        nonlocal stopping
        stopping=True
    signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop)
    atomic_json(folder/(name+'.json'),{'format':1,'pid':os.getpid(),'start_ticks':process_start(os.getpid()),'root':str(root),'spec_sha256':digest})
    try:
        while not stopping:
            child=subprocess.Popen(spec['command'],cwd=root,env={**os.environ,**spec['environment']},start_new_session=True)
            while child.poll() is None and not stopping:time.sleep(.1)
            if child.poll() is None:
                os.killpg(child.pid,signal.SIGTERM)
                try:child.wait(timeout=5)
                except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
            if not stopping:
                # Keep failures visible in the private log; bounded restart rate.
                print('Owned child exited:',name,child.returncode,flush=True)
                for _ in range(20):
                    if stopping:break
                    time.sleep(.1)
    finally:
        (folder/(name+'.json')).unlink(missing_ok=True);os.close(fd)


if __name__=='__main__':
    if len(sys.argv)!=6 or sys.argv[1]!='supervise':raise SystemExit('Internal supervisor invocation only')
    supervise(Path(sys.argv[2]).resolve(),sys.argv[3],int(sys.argv[4]),sys.argv[5])
