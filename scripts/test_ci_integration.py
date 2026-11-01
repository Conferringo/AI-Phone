"""Cargo test selection stays exact when a package gains integration-test bins."""
import json
import unittest
from ci_integration import assistant_test_executable


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
        self.assertEqual(assistant_test_executable(stream(*rows)),'fixture/assistant-tests')

    def test_zero_or_multiple_unit_test_executables_are_still_errors(self):
        unit=artifact('ai-phone-assistant',['bin'],True,'fixture/unit')
        for rows in [[],[artifact('ai-phone-assistant',['bin'],False,'fixture/normal')],[unit,unit]]:
            with self.subTest(rows=rows),self.assertRaises(ValueError):assistant_test_executable(stream(*rows))

    def test_matching_name_without_the_cargo_test_profile_and_bin_kind_is_rejected(self):
        for row in [artifact('ai-phone-assistant',['test'],True,'fixture/integration'),
                    artifact('ai-phone-assistant',['lib'],True,'fixture/lib'),
                    artifact('different',['bin'],True,'fixture/other'),
                    artifact('ai-phone-assistant',['bin'],True,None)]:
            with self.subTest(row=row),self.assertRaises(ValueError):assistant_test_executable(stream(row))
