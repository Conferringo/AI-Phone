"""Bounded allowlisted observations; never exports logs, paths, serials or env."""
import os
from pathlib import Path
import platform
import re
import selectors
import signal
import stat
import subprocess
import time

LIMIT = 8192
BOOT_PROPERTIES = {
    'sys.boot_completed': ('', '0', '1'),
    'dev.bootcomplete': ('', '0', '1'),
    'init.svc.bootanim': ('', 'running', 'stopped', 'restarting'),
    'init.svc.zygote': ('', 'running', 'stopped', 'restarting'),
}


def regular_reader(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    if not stat.S_ISREG(os.fstat(descriptor).st_mode):
        os.close(descriptor)
        raise ValueError('not regular')
    return os.fdopen(descriptor, 'rb')


def bounded(path, limit=LIMIT):
    with regular_reader(path) as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError('budget')
    return data.decode('utf-8', errors='strict')


def integer(value):
    if re.fullmatch(r'[0-9]{1,20}', value or '') is None:
        return None
    number = int(value)
    return number if number < 2**63 else None


def pairs(text, allowed):
    result = {}
    for line in text.splitlines():
        fields = line.replace(':', ' ').split()
        if len(fields) >= 2 and fields[0] in allowed:
            result[fields[0]] = integer(fields[1])
    return result


def memory_events(base):
    fields = ('oom', 'oom_kill', 'oom_group_kill')
    return pairs(bounded(base / 'memory.events', 1024), fields)


def cgroup_memory(proc, cgroup):
    rows = bounded(proc / 'self/cgroup', 4096).splitlines()
    selected = next((line[3:] for line in rows if line.startswith('0::')), None)
    if selected is None or not selected.startswith('/') or '..' in Path(selected).parts:
        return {'available': False}
    base = (cgroup / selected.lstrip('/')).resolve()
    if not base.is_relative_to(cgroup.resolve()):
        return {'available': False}
    current = integer(bounded(base / 'memory.current', 64).strip())
    maximum = bounded(base / 'memory.max', 64).strip()
    cpu = bounded(base / 'cpu.max', 128).split()
    return {'available': True, 'current_bytes': current,
            'max_bytes': 'max' if maximum == 'max' else integer(maximum),
            'cpu_quota': 'max' if cpu[0] == 'max' else integer(cpu[0]),
            'cpu_period': integer(cpu[1]), 'events': memory_events(base)}


def resources(proc=Path('/proc'), cgroup=Path('/sys/fs/cgroup'), system=None):
    if (system or platform.system()) != 'Linux':
        return {'platform': 'unsupported'}
    result = {'platform': 'linux'}
    try:
        result['memory_kib'] = pairs(bounded(proc / 'meminfo'), ('MemTotal', 'MemAvailable', 'SwapFree'))
        result['affinity_cpus'] = len(os.sched_getaffinity(0))
        result['cgroup'] = cgroup_memory(proc, cgroup)
    except (OSError, ValueError, IndexError, AttributeError, UnicodeError):
        result['available'] = False
    return result


def child_identity(proc, parent):
    pids = bounded(proc / str(parent) / 'task' / str(parent) / 'children', 1024).split()
    if len(pids) > 16:
        return None
    for value in pids:
        pid = integer(value)
        if pid is None:
            continue
        folder = proc / str(pid)
        if folder.stat().st_uid != os.getuid():
            continue
        status = pairs(bounded(folder / 'status'), ('PPid',))
        if status.get('PPid') != parent:
            continue
        fields = bounded(folder / 'stat', 4096).rsplit(')', 1)[1].split()
        if fields[0] != 'Z':
            return (pid, fields[19])
    return None


def owned_process(root, proc=Path('/proc'), system=None, query=None):
    if (system or platform.system()) != 'Linux':
        return {'verified': False, 'platform': 'unsupported'}
    try:
        if query is None:
            from linux_services import unit_state
            query = unit_state
        owner = query(root, 'emulator')
        if not owner:
            return {'verified': False, 'supervisor_alive': False}
        identity = child_identity(proc, owner['pid'])
        return {'verified': True, 'supervisor_alive': True, 'child_alive': identity is not None,
                'identity': identity}
    except (OSError, ValueError, IndexError, KeyError, TypeError, AttributeError, UnicodeError):
        return {'verified': False, 'available': False}


def last_owned_exit(root, owned):
    # This is historical evidence only. Its generation cannot be established.
    if not owned.get('verified'):
        return None
    try:
        folder = owned_data(root) / 'services'
        path = folder / 'emulator.log'
        if folder.is_symlink():
            return None
        with regular_reader(path) as stream:
            stream.seek(0, 2)
            stream.seek(max(0, stream.tell() - LIMIT))
            lines = stream.read(LIMIT).splitlines()
        codes = [int(match[1]) for line in lines
                 if (match := re.fullmatch(rb'Owned child exited: emulator (-?[0-9]{1,3})', line))]
        return codes[-1] if codes else None
    except (OSError, ValueError):
        return None


def owned_data(root):
    root = Path(root)
    data = root / 'data'
    if root.is_symlink():
        raise ValueError('root alias')
    if data.is_symlink():
        expected = root.parent.parent / 'user/data'
        if root.parent.name != 'releases' or re.fullmatch('[0-9a-f]{64}', root.name) is None:
            raise ValueError('unrecognized installation')
        if expected.parent.is_symlink() or expected.is_symlink() or data.resolve() != expected.resolve():
            raise ValueError('foreign data alias')
    return data.resolve()


def process_report(root):
    owned = owned_process(root)
    return {key: owned[key] for key in ('verified', 'platform', 'available', 'supervisor_alive', 'child_alive') if key in owned} | {
        'last_owned_exit_code': last_owned_exit(root, owned), 'current_exit_code': None,
    }


def sampled_command(command, deadline, invoke, clock):
    remaining = deadline - clock()
    if remaining <= 0:
        return None
    return invoke(command, min(1.0, remaining))


def adb_state(result):
    if result is None:
        return 'unknown'
    if result.returncode == 0:
        value = result.stdout.strip()
        return value if value in ('device', 'offline', 'unauthorized') else 'unknown'
    text = result.stderr.lower()
    if 'not found' in text or 'no devices' in text:
        return 'missing'
    return next((name for name in ('offline', 'unauthorized') if name in text), 'unknown')


def boot_probes(adb, deadline, invoke, clock):
    result = sampled_command([*adb, 'get-state'], deadline, invoke, clock)
    output = {'state': adb_state(result), 'return_code': result.returncode if result else None, 'properties': {}}
    for name, allowed in BOOT_PROPERTIES.items():
        current = sampled_command([*adb, 'shell', 'getprop', name], deadline, invoke, clock)
        value = current.stdout.strip() if current and current.returncode == 0 else 'unknown'
        output['properties'][name] = value if value in allowed else 'unknown'
    return output


def acceleration(adb, deadline, invoke, clock, system):
    if system != 'Linux':
        return {'status': 'unsupported', 'return_code': None}
    emulator = Path(adb[0]).parent.parent / 'emulator/emulator'
    result = sampled_command([str(emulator), '-accel-check'], deadline, invoke, clock)
    usable = result is not None and result.returncode == 0
    return {'status': 'usable' if usable else 'unavailable' if result is not None else 'unknown',
            'return_code': result.returncode if result else None}


def event_delta(before, after):
    old = before.get('cgroup', {}).get('events', {})
    new = after.get('cgroup', {}).get('events', {})
    return {key: new[key] - old[key] for key in ('oom', 'oom_kill', 'oom_group_kill')
            if isinstance(old.get(key), int) and isinstance(new.get(key), int) and new[key] >= old[key]}


def collect(root, adb, phase, state, before, started, *, invoke=None, clock=time.monotonic):
    invoke = invoke or bounded_probe
    deadline = clock() + 8
    system = platform.system()
    after = resources()
    return {'format': 1, 'phase': phase, 'elapsed_ms': max(0, int((clock() - started) * 1000)),
            'observations': state['observations'], 'last_boot': state['last_boot'],
            'configuration': applied_configuration(root), 'process': process_report(root), 'acceleration': acceleration(adb, deadline, invoke, clock, system),
            'adb': boot_probes(adb, deadline, invoke, clock), 'resources_before': before,
            'resources_after': after, 'cgroup_oom_delta': event_delta(before, after),
            'oom_cause_confirmed': False}


def applied_configuration(root):
    result = {}
    try:
        text = bounded(owned_data(root) / 'android/avd/phone-local-01.avd/config.ini')
        allowed = {'hw.cpu.ncore': 'phone_cpus', 'hw.ramSize': 'phone_memory_mib'}
        for line in text.splitlines():
            key, separator, value = line.partition('=')
            if separator and key in allowed:
                result[allowed[key]] = integer(value)
    except (OSError, ValueError, UnicodeError):
        pass
    return result


def drain_probe(child, buffers, deadline):
    with selectors.DefaultSelector() as selector:
        for name in buffers:
            selector.register(getattr(child, name), selectors.EVENT_READ, name)
        while selector.get_map():
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return False
            for key, _ in selector.select(remaining):
                data = os.read(key.fileobj.fileno(), 1024)
                if not data:
                    selector.unregister(key.fileobj)
                else:
                    target = buffers[key.data]
                    target.extend(data[:max(0, 4096 - len(target))])
    return True


def bounded_probe(command, timeout):
    """Read-only probe owns only its child; retain at most 4 KiB per pipe."""
    try:
        child = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    except OSError:
        return None
    buffers = {'stdout': bytearray(), 'stderr': bytearray()}
    deadline = time.monotonic() + timeout
    expired = False
    try:
        expired = not drain_probe(child, buffers, deadline)
        if not expired:
            try:
                child.wait(timeout=max(.001, deadline - time.monotonic()))
            except subprocess.TimeoutExpired:
                expired = True
    finally:
        # Even an exited direct child can leave descendants holding its pipes.
        # This session/process group was created only for this diagnostic probe.
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait()
        child.stdout.close()
        child.stderr.close()
    if expired:
        return None
    return subprocess.CompletedProcess(command, child.returncode,
        buffers['stdout'].decode(errors='replace'), buffers['stderr'].decode(errors='replace'))
