import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from release import configure, default_profile
from runtime_config import runtime_env

class InstallationLanguageTests(unittest.TestCase):
    def test_language_is_the_first_prompt_and_drives_configuration(self):
        with tempfile.TemporaryDirectory() as name:
            root=Path(name); profile=default_profile(root);profile['java_home']='/fixture/java'
            dest=root/'data/deployment/profile.json';dest.parent.mkdir(parents=True);dest.write_text(json.dumps(profile))
            prompts=[]
            answers=iter(['6']+['']*9)
            def reply(prompt):prompts.append(prompt);return next(answers)
            with patch('builtins.input',side_effect=reply),patch('builtins.print'):configure(root)
            self.assertIn('Language',prompts[0]);self.assertIn('1 简体中文',prompts[0]);self.assertIn('6 Deutsch',prompts[0]);self.assertIn('8 Español',prompts[0])
            self.assertEqual(runtime_env(root)['AI_PHONE_DEFAULT_LOCALE'],'de')
            self.assertIn('Telefon',prompts[5]);self.assertIn('Arbeitsspeicher',prompts[6])
    def test_invalid_choice_retries_and_existing_preference_is_preserved(self):
        with tempfile.TemporaryDirectory() as name:
            root=Path(name);profile=default_profile(root);profile.update(java_home='/fixture/java',locale='ja')
            dest=root/'data/deployment/profile.json';dest.parent.mkdir(parents=True);dest.write_text(json.dumps(profile))
            with patch('builtins.input',side_effect=['../../bad','']+['']*9),patch('builtins.print'):configure(root)
            self.assertEqual(runtime_env(root)['AI_PHONE_DEFAULT_LOCALE'],'ja')
    def test_number_codes_and_bad_inputs_are_bounded(self):
        from install_language import choose_language
        for answer, expected in [('1','zh-CN'),('8','es'),('fr','fr'),('','ja')]:
            with self.subTest(answer=answer),patch('builtins.input',return_value=answer):
                self.assertEqual(choose_language('ja'),expected)
        with patch('builtins.input',side_effect=['0','9','１２','../../bad','en']),patch('builtins.print') as output:
            self.assertEqual(choose_language('ja'),'en')
            self.assertEqual(output.call_count,4)
    def test_eight_language_catalogs_have_identical_nonempty_keys(self):
        from install_language import CATALOGS
        self.assertEqual(set(CATALOGS),{'zh-CN','en','zh-TW','ja','ko','de','fr','es'})
        for labels in CATALOGS.values():
            self.assertEqual(set(labels),set(CATALOGS['en']))
            self.assertTrue(all(isinstance(v,str) and v for v in labels.values()))
