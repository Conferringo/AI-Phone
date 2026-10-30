#!/usr/bin/python3
"""Dunst notification hook. Durable guest-only inbox, never executes message text."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import tempfile
import time
import uuid


def record(folder, env):
    folder.mkdir(mode=0o700, parents=True, exist_ok=True)
    with (folder / 'notifications.lock').open('a') as lock:
        os.chmod(lock.name, 0o600)
        fcntl.flock(lock, fcntl.LOCK_EX)
        path = folder / 'notifications.json'
        events = json.loads(path.read_text()) if path.exists() else []
        event = {
            'event_id': hashlib.sha256(uuid.uuid4().bytes).hexdigest(),
            'notification_key': 'desktop:' + hashlib.sha256(env.get('DBUS_SESSION_BUS_ADDRESS', '').encode()).hexdigest()[:12] + ':' + env.get('DUNST_ID', '')[:80],
            'package_name': (env.get('DUNST_DESKTOP_ENTRY') or env.get('DUNST_APP_NAME', ''))[:240],
            'app_name': env.get('DUNST_APP_NAME', '')[:240],
            'title': env.get('DUNST_SUMMARY', '')[:1000],
            'text': env.get('DUNST_BODY', '')[:12000],
            'posted_at_ms': time.time_ns() // 1_000_000,
        }
        events.append(event)
        encoded = [json.dumps(item, ensure_ascii=False) for item in events[-256:]]
        size = sum(len(item.encode()) for item in encoded) + len(encoded) + 1
        while size > 2_000_000 and len(encoded) > 1:
            size -= len(encoded.pop(0).encode()) + 1
        fd, name = tempfile.mkstemp(prefix='notifications-', dir=folder)
        try:
            with os.fdopen(fd, 'w') as out:
                out.write('[' + ','.join(encoded) + ']')
                out.flush()
                os.fsync(out.fileno())
            os.replace(name, path)
        finally:
            if os.path.exists(name):
                os.unlink(name)


if __name__ == '__main__':
    record(Path.home() / '.aiphone', os.environ)
