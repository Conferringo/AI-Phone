"""Container absence is a successful query, never an interpretation of stderr."""
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from client_host import verify_container_ownership, write_container_wrapper


NAMES = ('ai-phone-postgres-1', 'ai-phone-redis-1', 'ai-phone-desktop')
IDENTIFIER = 'a' * 64


def result(stdout='', returncode=0, stderr=''):
    return subprocess.CompletedProcess([], returncode, stdout=stdout, stderr=stderr)


def owned(root, identifier=IDENTIFIER, running=False):
    return {'id': identifier, 'running': running, 'labels': {
        'com.docker.compose.project': 'ai-phone',
        'com.docker.compose.project.working_dir': str(root),
    }}


class ContainerOwnershipTests(unittest.TestCase):
    def test_real_shell_wrapper_accepts_successful_empty_query_despite_stderr(self):
        # Real production wrapper; only its Lima executable is a local shell fixture.
        # Additional stderr is synthetic evidence, not a claim about the lost CI stderr.
        with tempfile.TemporaryDirectory(prefix='container owner ') as temporary:
            root = Path(temporary)
            journal = root / 'queries.jsonl'
            lima = root / 'fake lima'
            program = (
                'import json,sys\nfrom pathlib import Path\n'
                'args=sys.argv[sys.argv.index("docker")+1:]\n'
                f'with Path({str(journal)!r}).open("a") as stream:stream.write(json.dumps(args)+"\\n")\n'
                'if args[:2]==["container","ls"]:\n'
                ' print("fixture wrapper diagnostic",file=sys.stderr)\n'
                ' sys.exit(0)\n'
                'print("Error: No such object: "+args[-1],file=sys.stderr)\n'
                'print("fixture wrapper: command exited with status 1",file=sys.stderr)\n'
                'sys.exit(1)\n'
            )
            lima.write_text('#!/bin/sh\nexec ' + shlex.quote(sys.executable) + ' -c ' + shlex.quote(program) + ' "$@"\n')
            lima.chmod(0o755)
            wrapper = write_container_wrapper(root, root, lima)
            with patch.dict(os.environ, {'PATH': str(wrapper)}):
                self.assertEqual(verify_container_ownership(root), {})
            commands = [json.loads(line) for line in journal.read_text().splitlines()]
            self.assertEqual(commands, [
                ['container', 'ls', '--all', '--filter', 'name=^/' + name + '$', '--no-trunc', '--format', '{{.ID}}']
                for name in NAMES
            ])

    def test_query_failure_never_proves_absence_from_no_such_stderr(self):
        for stderr in ('No such object: ai-phone-postgres-1',
                       'Error: No such object: ai-phone-postgres-1',
                       'Error: No such object: ai-phone-postgres-1\nwrapper exit 1',
                       'Cannot connect to daemon'):
            with self.subTest(stderr=stderr):
                calls = []
                def query(argv, **_):
                    calls.append(argv)
                    return result(returncode=1, stderr=stderr)
                with self.assertRaisesRegex(ValueError, 'could not be verified'):
                    verify_container_ownership(Path('/owned'), query)
                self.assertEqual(len(calls), 1)
                self.assertEqual(calls[0][:3], ['docker', 'container', 'ls'])

    def test_single_full_id_is_inspected_as_a_container_not_a_name_or_image(self):
        root = Path('/owned')
        for running in (False, True):
            with self.subTest(running=running):
                calls = []
                def query(argv, **_):
                    calls.append(argv)
                    if argv[:3] == ['docker', 'container', 'ls']:
                        name_filter = argv[argv.index('--filter') + 1]
                        return result(IDENTIFIER + '\n' if name_filter == 'name=^/ai-phone-desktop$' else '')
                    self.assertEqual(argv[:4], ['docker', 'container', 'inspect', '--format'])
                    self.assertEqual(argv[-1], IDENTIFIER)
                    return result(json.dumps(owned(root, running=running)))
                self.assertEqual(verify_container_ownership(root, query), {
                    'ai-phone-desktop': {'id': IDENTIFIER, 'running': running},
                })
                self.assertEqual(len(calls), 4)

    def test_ambiguous_or_non_full_list_ids_are_refused_without_inspection(self):
        for output in ('a' * 12, 'A' * 64, 'not-an-id', IDENTIFIER + '\n' + IDENTIFIER,
                       IDENTIFIER + '\n' + 'b' * 64, IDENTIFIER + ' diagnostic'):
            with self.subTest(output=output):
                calls = []
                def query(argv, **_):
                    calls.append(argv)
                    return result(output)
                with self.assertRaisesRegex(ValueError, 'could not be verified'):
                    verify_container_ownership(Path('/owned'), query)
                self.assertEqual(len(calls), 1)
                self.assertEqual(calls[0][:3], ['docker', 'container', 'ls'])

    def test_failed_or_replaced_inspection_never_falls_back_to_a_name(self):
        root = Path('/owned')
        cases = [result(returncode=1, stderr='No such container: ' + IDENTIFIER),
                 result(json.dumps(owned(root, identifier='b' * 64))),
                 result(json.dumps(owned(root, running='false'))),
                 result(json.dumps(owned(root, running=None))),
                 result(json.dumps(owned(Path('/foreign')))),
                 result('{}'), result('[]'), result('not-json')]
        for inspection in cases:
            with self.subTest(inspection=inspection):
                calls = []
                def query(argv, **_):
                    calls.append(argv)
                    return result(IDENTIFIER) if len(calls) == 1 else inspection
                with self.assertRaises(ValueError): verify_container_ownership(root, query)
                self.assertEqual(len(calls), 2)
                self.assertEqual(calls[1][:4], ['docker', 'container', 'inspect', '--format'])
                self.assertEqual(calls[1][-1], IDENTIFIER)


if __name__ == '__main__': unittest.main()
