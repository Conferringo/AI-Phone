"""Isolated guest inbox persistence tests. Never touches a running desktop."""
from concurrent.futures import ThreadPoolExecutor
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('notification_record', Path(__file__).resolve().parents[1] / 'infra/desktop/notification-record.py')
hook = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hook)


class DesktopNotificationTests(unittest.TestCase):
    def test_parallel_notifications_are_not_lost_or_world_readable(self):
        with tempfile.TemporaryDirectory() as root:
            folder = Path(root) / 'inbox'
            def post(index):
                hook.record(folder, {'DUNST_ID': str(index), 'DUNST_BODY': f'message {index}'})
            with ThreadPoolExecutor(max_workers=8) as pool:
                list(pool.map(post, range(40)))
            events = json.loads((folder / 'notifications.json').read_text())
            self.assertEqual(len(events), 40)
            self.assertEqual(len({e['event_id'] for e in events}), 40)
            self.assertEqual({e['text'] for e in events}, {f'message {i}' for i in range(40)})
            for path in folder.iterdir():
                self.assertEqual(path.stat().st_mode & 0o077, 0)

    def test_inbox_is_bounded_and_replacements_keep_the_source_key(self):
        with tempfile.TemporaryDirectory() as root:
            folder = Path(root)
            for index in range(270):
                hook.record(folder, {'DUNST_ID': 'same-source', 'DUNST_BODY': str(index), 'DBUS_SESSION_BUS_ADDRESS': 'fixture-session'})
            events = json.loads((folder / 'notifications.json').read_text())
            self.assertEqual(len(events), 256)
            self.assertEqual(events[0]['text'], '14')
            self.assertEqual(events[-1]['text'], '269')
            self.assertEqual(len({e['notification_key'] for e in events}), 1)
            self.assertEqual(len({e['event_id'] for e in events}), 256)
            self.assertFalse(list(folder.glob('notifications-*')))

    def test_notification_content_is_data_and_has_limits(self):
        with tempfile.TemporaryDirectory() as root:
            folder = Path(root)
            hook.record(folder, {'DUNST_BODY': '$(touch should-never-exist)' + 'x' * 20000, 'DUNST_SUMMARY': 's' * 2000, 'DUNST_DESKTOP_ENTRY': 'a' * 500})
            event = json.loads((folder / 'notifications.json').read_text())[0]
            self.assertEqual(len(event['text']), 12000)
            self.assertEqual(len(event['title']), 1000)
            self.assertEqual(len(event['package_name']), 240)
            self.assertEqual({p.name for p in folder.iterdir()}, {'notifications.json', 'notifications.lock'})

    def test_multibyte_messages_stay_under_the_transport_byte_budget(self):
        with tempfile.TemporaryDirectory() as root:
            folder = Path(root)
            for index in range(50):
                hook.record(folder, {'DUNST_ID': str(index), 'DUNST_BODY': '🐳' * 12000})
            path = folder / 'notifications.json'
            events = json.loads(path.read_text())
            self.assertLessEqual(path.stat().st_size, 2_000_000)
            self.assertLess(len(events), 50)
            self.assertTrue(events[-1]['notification_key'].endswith(':49'))


if __name__ == '__main__':
    unittest.main()
