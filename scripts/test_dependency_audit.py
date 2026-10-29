import unittest
from audit_dependencies import findings


class AuditTests(unittest.TestCase):
    def test_same_package_name_does_not_hide_a_different_active_version(self):
        item = {'package': {'name': 'library', 'version': '1.2.0'},
                'advisory': {'id': 'RUSTSEC-test', 'title': 'Test', 'url': 'https://rustsec.org'}}
        report = {'vulnerabilities': {'list': [item]}, 'warnings': {'unsound': [item]}}
        rows = findings(report, {'mac': {('library', '1.3.0')}, 'linux': {('library', '1.2.0')}})
        self.assertEqual(len(rows), 2)
        for row in rows:
            self.assertEqual(row['active_targets'], ['linux'])
            self.assertTrue(row['blocking_when_active'])

    def test_inactive_lock_entries_and_maintenance_warnings_remain_visible(self):
        item = {'package': {'name': 'unused', 'version': '1.0'},
                'advisory': {'id': 'RUSTSEC-test', 'title': 'Test', 'url': 'https://rustsec.org'}}
        rows = findings({'vulnerabilities': {'list': [item]}, 'warnings': {'unmaintained': [item]}}, {'mac': set()})
        self.assertEqual(len(rows), 2)
        self.assertEqual(rows[0]['active_targets'], [])
        self.assertTrue(rows[0]['blocking_when_active'])
        self.assertFalse(rows[1]['blocking_when_active'])


if __name__ == '__main__': unittest.main()
