"""App-owned Linux supervisors. A receipt never authorizes killing a reused PID."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import stat
import subprocess
import sys
import threading
import time
from database import atomic_json
from prebuilt_services import supervisor_command

_SPEC_SERVICES = ('emulator', 'runner', 'media', 'assistant', 'channels', 'web', 'lan')
_SPEC_ENV_FIELDS = ('PATH', 'JAVA_HOME', 'AI_PHONE_JAVA_HOME', 'AI_PHONE_SDK_ROOT',
                    'AI_PHONE_PROJECT_ROOT', 'AI_PHONE_DATA_ROOT')


def folder_for(root, name):
    if not re.fullmatch(r'[a-z][a-z0-9-]{0,39}',name):raise ValueError('Invalid service name')
    return Path(root).resolve()/'data/services'


def process_start(pid):
    try:
        path=Path('/proc')/str(pid)
        if path.stat().st_uid!=os.getuid():return None
        fields=(path/'stat').read_text().rsplit(')',1)[1].split()
        return None if fields[0]=='Z' else fields[19]
    except (OSError,ValueError,IndexError):return None


def read_receipt(folder, name):
    """Open existing private state once; inspection never creates directories."""
    directory = os.open(folder, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        descriptor = os.open(name+'.json', os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW, dir_fd=directory)
        with os.fdopen(descriptor, 'rb') as stream:
            if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
                return None
            data = stream.read(8193)
    finally:
        os.close(directory)
    if len(data) > 8192:
        return None
    return json.loads(data)


def unit_state(root, name):
    try:
        value=read_receipt(folder_for(root,name),name)
        if not isinstance(value,dict):return None
        if value.get('root')!=str(Path(root).resolve()) or type(value.get('pid')) is not int:return None
        start=process_start(value['pid'])
        if start is None or start!=value.get('start_ticks'):return None
        args=(Path('/proc')/str(value['pid'])/'cmdline').read_bytes().split(b'\0')
        return value if supervisor_matches(value,root,name,args) else None
    except (OSError,ValueError):return None


def supervisor_matches(value,root,name,args):
    if value.get('supervisor') != 'rust-v1':
        return str(Path(__file__).resolve()).encode() in args and name.encode() in args
    expected=[value.get('program'), '--supervise', str(Path(root).resolve()), name,
              value.get('spec_sha256'), value.get('spec_content_sha256')]
    return (all(isinstance(part,str) for part in expected)
            and args == [part.encode() for part in expected]+[b'']
            and os.readlink(Path('/proc')/str(value['pid'])/'exe') == value['program'])


def spec_shape(value):
    return (isinstance(value, dict) and set(value) == {'command', 'environment'}
            and isinstance(value['command'], list) and isinstance(value['environment'], dict))


def verified_spec(folder, name, receipt):
    value = read_receipt(folder, name + '.spec')
    if not spec_shape(value):
        return None
    if not all(isinstance(part, str) for part in value['command']):
        return None
    if not all(isinstance(text, str) for text in value['environment'].values()):
        return None
    digest = hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()
    return value if digest == receipt.get('spec_sha256') else None


def spec_conflict_summary(folder, name, receipt, requested):
    summary = {'format': 1, 'service': name if name in _SPEC_SERVICES else 'other',
               'evidence': 'unverified_spec'}
    try:
        previous = verified_spec(folder, name, receipt)
    except (OSError, ValueError, TypeError):
        return summary
    if previous is None:
        return summary
    before, after = previous['environment'], requested['environment']
    missing = object()
    changed = {key for key in before.keys() | after.keys()
               if before.get(key, missing) != after.get(key, missing)}
    summary.update(evidence='verified_spec', command_changed=previous['command'] != requested['command'],
                   environment_fields=[key for key in _SPEC_ENV_FIELDS if key in changed],
                   other_environment_changes=len(changed.difference(_SPEC_ENV_FIELDS)))
    return summary


def report_spec_conflict(folder, name, receipt, requested):
    try:
        summary = spec_conflict_summary(folder, name, receipt, requested)
        print('AI_PHONE_SERVICE_SPEC ' + json.dumps(summary, sort_keys=True), file=sys.stderr, flush=True)
    except Exception:
        # Evidence is best effort; the original mismatch still refuses this resume.
        return


def start_unit(root, name, command, environment):
    root=Path(root).resolve();folder=folder_for(root,name)
    folder.mkdir(parents=True,exist_ok=True,mode=0o700)
    spec={'command':list(map(str,command)),'environment':environment}
    digest=hashlib.sha256(json.dumps(spec,sort_keys=True).encode()).hexdigest()
    fd=os.open(folder/(name+'.lock'),os.O_RDWR|os.O_CREAT,0o600)
    try:
        try:fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:
            deadline=time.monotonic()+5
            while not (old:=unit_state(root,name)) and time.monotonic()<deadline:time.sleep(.05)
            if not old:raise ValueError('Service ownership is unknown; no process was changed')
            if old.get('spec_sha256')!=digest:
                report_spec_conflict(folder, name, old, spec)
                raise ValueError('A running task has a different service configuration; stop the owned installation before upgrading')
            return False
        atomic_json(folder/(name+'.spec.json'),spec)
        content=hashlib.sha256((folder/(name+'.spec.json')).read_bytes()).hexdigest()
        program=supervisor_command(root)
        log=os.open(folder/(name+'.log'),os.O_WRONLY|os.O_CREAT|os.O_APPEND,0o600)
        try:
            child=subprocess.Popen([*program,'--supervise',str(root),name,digest,content],
                                   cwd=root,stdin=fd,stdout=log,stderr=log,start_new_session=True)
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
    while process_start(value['pid'])==value['start_ticks'] and time.monotonic()<deadline:time.sleep(.05)
    if process_start(value['pid'])==value['start_ticks']:raise ValueError('Owned service did not stop; no PID was force-killed')
    return True
