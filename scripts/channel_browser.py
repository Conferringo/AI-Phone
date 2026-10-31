"""Puppeteer executable adapter for the bundled Linux Chromium image."""
from contextlib import contextmanager
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from browser_engine import DockerEngine
from browser_lease import BrowserContext,BrowserLease,BrowserOwnerChanged,HEX
from browser_pipe import run_relay


def launch_arguments(context,args):
    if len(args)>64 or sum(map(len,args))>16384:raise ValueError('Browser argument budget exceeded')
    profiles=[arg.split('=',1)[1] for arg in args if arg.startswith('--user-data-dir=')]
    if len(profiles)!=1 or Path(profiles[0]).resolve()!=context.profile.resolve():raise ValueError('Browser profile differs from its owner')
    if '--remote-debugging-pipe' not in args:raise ValueError('Browser requires its private pipe')
    return guest_arguments(args)


def guest_arguments(args):
    result=[]
    for arg in args:
        if not arg.startswith('--') or '\0' in arg:raise ValueError('Invalid browser argument')
        if arg.startswith('--remote-debugging-') and arg!='--remote-debugging-pipe':raise ValueError('Browser debug ports are forbidden')
        if arg=='--remote-debugging-pipe' or arg.startswith('--user-data-dir='):continue
        result.append(arg)
    return result


@contextmanager
def owned_lease(context,engine,owner,*,wait=0):
    deadline=time.monotonic()+wait
    while True:
        lease=BrowserLease(context,engine,owner=owner)
        try:lease.__enter__();break
        except BlockingIOError:
            if time.monotonic()>=deadline:raise
            time.sleep(0.05)
    try:yield lease
    finally:lease.__exit__(None,None,None)


def wait_attached(child):
    if child is None:return
    child.stdin.close()
    try:child.wait(timeout=3)
    except subprocess.TimeoutExpired:
        child.terminate()
        try:child.wait(timeout=1)
        except subprocess.TimeoutExpired:child.kill();child.wait()


def launch(context,engine,owner,args):
    args=launch_arguments(context,args)
    for fd in (3,4):os.fstat(fd)
    if os.name=='nt':
        import msvcrt
        for fd in (3,4):msvcrt.setmode(fd,os.O_BINARY)
    with owned_lease(context,engine,owner) as lease:
        lease.recover();child=None
        try:
            identity=lease.create(args);child=engine.attach(identity)
            run_relay(child,[(3,child.stdin.fileno()),(child.stdout.fileno(),4)])
        finally:
            wait_attached(child)
            lease.recover(expected_owner=owner)


def manage(context,engine,owner,mode):
    try:
        with owned_lease(context,engine,owner,wait=8) as lease:
            if mode=='--erase':lease.erase_profile(expected_owner=owner)
            else:lease.recover(expected_owner=owner)
    except BrowserOwnerChanged:
        if mode=='--erase':raise
        return {'stopped':True,'superseded':True}
    return {'stopped':True,'erased':mode=='--erase'}


def main(args,env):
    root=Path(env.get('AI_PHONE_PROJECT_ROOT',Path(__file__).resolve().parent.parent))
    image=env.get('AI_PHONE_DESKTOP_IMAGE','')
    engine=DockerEngine(network=env.get('AI_PHONE_BROWSER_NETWORK','bridge'))
    if args==['--check']:
        identity=BrowserContext(root,'bootstrap-check',image).image_id
        print(json.dumps({'available':True,'version':engine.check(identity)}));return
    context=BrowserContext(root,env.get('AI_PHONE_BROWSER_SESSION',''),image);owner=env.get('AI_PHONE_BROWSER_OWNER','')
    if HEX.fullmatch(owner) is None:raise ValueError('Browser launch owner missing')
    if args in (['--stop'],['--erase']):print(json.dumps(manage(context,engine,owner,args[0])))
    else:launch(context,engine,owner,args)


if __name__=='__main__':
    try:main(sys.argv[1:],os.environ)
    except (ValueError,OSError,RuntimeError,subprocess.SubprocessError):
        print('AIPHONE_BROWSER_UNAVAILABLE: browser ownership or container operation failed',file=sys.stderr)
        raise SystemExit(1)
