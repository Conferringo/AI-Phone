"""Required CI checks must execute their tests, including fixture-gated suites."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT=Path(__file__).absolute().parent


class RequiredTestsTests(unittest.TestCase):
    def run_case(self,body):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'required_fixture.py';path.write_text('import unittest\n'+body)
            environment={**os.environ,'PYTHONPATH':directory,'PYTHONDONTWRITEBYTECODE':'1'}
            return subprocess.run([sys.executable,str(ROOT/'required_tests.py'),'required_fixture'],
                                  cwd=directory,env=environment,capture_output=True,text=True,timeout=10)

    def test_passing_suite_executes_and_returns_success(self):
        result=self.run_case('class Fixture(unittest.TestCase):\n def test_pass(self): self.assertEqual(2+2,4)\n')
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertIn('Ran 1 test',result.stderr)

    def test_skipped_required_case_is_a_failure_even_with_a_passing_case(self):
        result=self.run_case('class Fixture(unittest.TestCase):\n def test_pass(self): pass\n @unittest.skip("missing fixture")\n def test_skip(self): pass\n')
        self.assertNotEqual(result.returncode,0)
        self.assertIn('missing fixture',result.stderr)
        self.assertIn('Required tests did not all execute',result.stderr)

    def test_empty_suite_cannot_succeed(self):
        result=self.run_case('class Fixture(unittest.TestCase): pass\n')
        self.assertNotEqual(result.returncode,0)
        self.assertIn('Required tests did not all execute',result.stderr)

    def test_assertion_and_import_failures_remain_failures(self):
        for body,expected in [('class Fixture(unittest.TestCase):\n def test_fail(self): self.fail("fixture failure")\n','FAILED'),
                              ('raise RuntimeError("fixture import failure")\n','RuntimeError: fixture import failure')]:
            with self.subTest(body=body):
                result=self.run_case(body)
                self.assertNotEqual(result.returncode,0)
                self.assertIn(expected,result.stderr)
