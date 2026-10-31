"""Bounded binary relay, also executed inside the pinned Linux browser image."""
import os
import signal
import subprocess
import sys
import threading


def write_all(fd,data):
    view=memoryview(data)
    while view:
        count=os.write(fd,view)
        if count<=0:raise BrokenPipeError('Browser pipe closed')
        view=view[count:]


def relay(source,destination,stop):
    try:
        while data:=os.read(source,65536):write_all(destination,data)
    except OSError:pass
    finally:stop.set()


def run_relay(child,pairs):
    stop=threading.Event()
    for event in (signal.SIGTERM,signal.SIGINT):signal.signal(event,lambda *_:stop.set())
    for source,destination in pairs:
        threading.Thread(target=relay,args=(source,destination,stop),daemon=True).start()
    while not stop.wait(0.05):
        if child.poll() is not None:break


def high_pipe():
    import fcntl
    reader,writer=os.pipe()
    duplicate=None
    try:
        duplicate=fcntl.fcntl(reader,fcntl.F_DUPFD_CLOEXEC,10)
        return duplicate,fcntl.fcntl(writer,fcntl.F_DUPFD_CLOEXEC,10)
    except BaseException:
        if duplicate is not None:os.close(duplicate)
        raise
    finally:os.close(reader);os.close(writer)


def reserve_debug_fds():
    for fd in (3,4):
        owned=os.open('/dev/null',os.O_RDWR)
        if owned!=fd:os.dup2(owned,fd);os.close(owned)


def stop_guest(child):
    # A normal Browser.close may close its CDP pipe before profile flush finishes.
    try:child.wait(timeout=3);return
    except subprocess.TimeoutExpired:pass
    try:os.killpg(child.pid,signal.SIGTERM)
    except ProcessLookupError:child.wait();return
    try:child.wait(timeout=1)
    except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()


def guest(args):
    reserve_debug_fds()
    incoming,writer=high_pipe();reader,outgoing=high_pipe()
    os.dup2(incoming,3);os.dup2(outgoing,4)
    command=['/usr/bin/chromium','--headless=new','--no-sandbox','--remote-debugging-pipe','--user-data-dir=/profile','--disable-background-networking',*args,'about:blank']
    child=subprocess.Popen(command,pass_fds=(3,4),stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
    for fd in (3,4,incoming,outgoing):os.close(fd)
    try:run_relay(child,[(0,writer),(reader,1)])
    finally:os.close(writer);stop_guest(child);os.close(reader)


if __name__=='__main__':
    if len(sys.argv)<2 or sys.argv[1]!='--guest':raise SystemExit('Browser pipe requires its guest entry point')
    guest(sys.argv[2:])
