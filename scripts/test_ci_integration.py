"""Cargo test selection stays exact when a package gains integration-test bins."""
import json
import os
import unittest
from unittest.mock import patch
from ci_integration import assistant_executable, service_binary


def artifact(name,kind,testing,executable):
    return {'reason':'compiler-artifact','target':{'name':name,'kind':kind},
            'profile':{'test':testing},'executable':executable}


def stream(*rows):
    return '\n'.join(json.dumps(row) for row in rows)


class CargoTestSelectionTests(unittest.TestCase):
    def test_ordinary_binary_and_cli_suite_do_not_replace_assistant_unit_tests(self):
        rows=[artifact('ai-phone-assistant',['bin'],False,'fixture/assistant'),
              artifact('ai-phone-assistant',['bin'],True,'fixture/assistant-tests'),
              artifact('device_setup_cli',['test'],True,'fixture/cli-tests'),
              {'reason':'build-finished','success':True}]
        self.assertEqual(assistant_executable(stream(*rows),testing=True),'fixture/assistant-tests')
        self.assertEqual(assistant_executable(stream(*rows),testing=False),'fixture/assistant')

    def test_zero_or_multiple_unit_test_executables_are_still_errors(self):
        unit=artifact('ai-phone-assistant',['bin'],True,'fixture/unit')
        for rows in [[],[artifact('ai-phone-assistant',['bin'],False,'fixture/normal')],[unit,unit]]:
            with self.subTest(rows=rows),self.assertRaises(ValueError):assistant_executable(stream(*rows),testing=True)

    def test_matching_name_without_the_cargo_test_profile_and_bin_kind_is_rejected(self):
        for row in [artifact('ai-phone-assistant',['test'],True,'fixture/integration'),
                    artifact('ai-phone-assistant',['lib'],True,'fixture/lib'),
                    artifact('different',['bin'],True,'fixture/other'),
                    artifact('ai-phone-assistant',['bin'],True,None)]:
            with self.subTest(row=row),self.assertRaises(ValueError):assistant_executable(stream(row),testing=True)

    def test_normal_executable_must_be_unique_and_have_the_non_test_profile(self):
        binary=artifact('ai-phone-assistant',['bin'],False,'custom target/debug/assistant')
        for rows in [[],[artifact('ai-phone-assistant',['bin'],True,'fixture/tests')],
                     [artifact('ai-phone-assistant',['bin'],None,'fixture/unknown')],[binary,binary]]:
            with self.subTest(rows=rows),self.assertRaises(ValueError):assistant_executable(stream(*rows),testing=False)
        selected=assistant_executable(stream(binary),testing=False)
        with patch.dict(os.environ,{'AI_PHONE_TEST_ASSISTANT_BINARY':selected}):
            self.assertEqual(service_binary(),'custom target/debug/assistant')

    def test_fixture_never_falls_back_to_a_stale_default_target(self):
        with patch.dict(os.environ,{},clear=True),self.assertRaisesRegex(ValueError,'AI_PHONE_TEST_ASSISTANT_BINARY'):
            service_binary()
