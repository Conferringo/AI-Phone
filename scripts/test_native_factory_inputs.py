"""Exact build-input identity and transport, without GitHub or user state."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from artifact_io import digest
import native_factory_inputs as inputs


class NativeFactoryInputsTests(unittest.TestCase):
    def fixture(self, root):
        (root/'config').mkdir()
        metadata, content = {}, {}
        for role, asset, config in [('linux-host', '11', 'linux-host-runtime.json'),
                                    ('linux-ocr', '12', 'linux-ocr.json')]:
            data = ('fixed '+role).encode()
            source = root/(role+'.source');source.write_bytes(data)
            archive = {'file': 'linux-x64-'+('host' if role=='linux-host' else 'ocr')+'.tar.gz',
                       'size_bytes': len(data), 'sha256': digest(source)}
            (root/'config'/config).write_text(json.dumps({'format':1,'host':'linux-x64','archive':archive,'redistribution_review_complete':False}))
            metadata[asset] = {'name':archive['file'],'size':len(data),'digest':'sha256:'+archive['sha256']}
            content[asset] = data
        return metadata, content

    def test_download_admits_both_exact_archives_and_reuses_verified_files(self):
        with tempfile.TemporaryDirectory(prefix='native inputs ') as temporary:
            root=Path(temporary);metadata,content=self.fixture(root)
            def inspect(command,**_):return json.dumps(metadata[command[2].rsplit('/',1)[1]])
            def download(command,**kwargs):
                self.assertEqual(command[-2:],['--header','Accept: application/octet-stream'])
                kwargs['stdout'].write(content[command[2].rsplit('/',1)[1]])
            with patch.object(inputs.subprocess,'check_output',side_effect=inspect) as read,patch.object(inputs.subprocess,'run',side_effect=download) as fetch:
                folder=inputs.acquire(root,{'linux-host':'11','linux-ocr':'12'},'example/project')
                self.assertEqual(read.call_count,2);self.assertEqual(fetch.call_count,2)
                self.assertEqual(inputs.acquire(root,{'linux-host':'11','linux-ocr':'12'},'example/project'),folder)
                self.assertEqual(fetch.call_count,2)
            self.assertEqual({p.name for p in folder.iterdir()},{value['name'] for value in metadata.values()})
            with patch.dict(os.environ,{'AI_PHONE_FACTORY_ARCHIVES':str(folder)}):
                self.assertEqual(inputs.archive_directory(root,'linux-host'),folder)
                self.assertEqual(inputs.archive_directory(root,'linux-ocr'),folder)
            # No publication/review decision is changed by using an input.
            self.assertFalse(json.loads((root/'config/linux-ocr.json').read_text())['redistribution_review_complete'])

    def test_wrong_asset_metadata_is_rejected_before_body_download(self):
        for field,value in [('name','foreign.tar.gz'),('size',999),('digest','sha256:'+'0'*64)]:
            with self.subTest(field=field),tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary);metadata,_=self.fixture(root);bad={**metadata['11'],field:value}
                with patch.object(inputs.subprocess,'check_output',return_value=json.dumps(bad)),patch.object(inputs.subprocess,'run') as fetch:
                    with self.assertRaisesRegex(ValueError,'pin'):inputs.acquire(root,{'linux-host':'11','linux-ocr':'12'},'example/project')
                    fetch.assert_not_called()

    def test_bad_body_or_transport_never_publishes_a_partial_archive(self):
        for failure in ('bytes','http'):
            with self.subTest(failure=failure),tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary);metadata,_=self.fixture(root)
                def download(command,**kwargs):
                    kwargs['stdout'].write(b'bad bytes')
                    if failure=='http':raise subprocess.CalledProcessError(1,command)
                with patch.object(inputs.subprocess,'check_output',return_value=json.dumps(metadata['11'])),patch.object(inputs.subprocess,'run',side_effect=download):
                    with self.assertRaises((ValueError,subprocess.CalledProcessError)):inputs.acquire(root,{'linux-host':'11','linux-ocr':'12'},'example/project')
                self.assertEqual(list((root/'.tools/factory-native-inputs').iterdir()),[])

    def test_unconfigured_source_path_has_no_github_or_environment_side_effect(self):
        with tempfile.TemporaryDirectory() as temporary:
            env=Path(temporary)/'github-env'
            with patch.dict(os.environ,{'AI_PHONE_NATIVE_HOST_ASSET':'','AI_PHONE_NATIVE_OCR_ASSET':'','GITHUB_ENV':str(env)}),patch.object(inputs.subprocess,'check_output') as read:
                inputs.main();read.assert_not_called()
            self.assertFalse(env.exists())

    def test_later_steps_only_receive_the_directory_after_both_downloads_succeed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);metadata,content=self.fixture(root);environment=root/'github-env'
            def inspect(command,**_):return json.dumps(metadata[command[2].rsplit('/',1)[1]])
            def download(command,**kwargs):
                asset=command[2].rsplit('/',1)[1]
                if asset=='12':raise subprocess.CalledProcessError(1,command)
                kwargs['stdout'].write(content[asset])
            env={'AI_PHONE_NATIVE_HOST_ASSET':'11','AI_PHONE_NATIVE_OCR_ASSET':'12','GITHUB_REPOSITORY':'example/project','GITHUB_ENV':str(environment)}
            with patch.object(inputs,'ROOT',root),patch.dict(os.environ,env),patch.object(inputs.subprocess,'check_output',side_effect=inspect),patch.object(inputs.subprocess,'run',side_effect=download):
                with self.assertRaises(subprocess.CalledProcessError):inputs.main()
            self.assertFalse(environment.exists())
            self.assertTrue((root/'.tools/factory-native-inputs'/metadata['11']['name']).is_file())

    def test_partial_configuration_and_corrupt_explicit_inputs_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);metadata,_=self.fixture(root)
            for assets,repository in [({'linux-host':'11','linux-ocr':''},'example/project'),({'linux-host':'../11','linux-ocr':'12'},'example/project'),({'linux-host':'11','linux-ocr':'12'},'../foreign')]:
                with self.subTest(assets=assets,repository=repository),patch.object(inputs.subprocess,'check_output') as read:
                    with self.assertRaises(ValueError):inputs.acquire(root,assets,repository)
                    read.assert_not_called()
            folder=root/'inputs';folder.mkdir()
            with patch.dict(os.environ,{'AI_PHONE_FACTORY_ARCHIVES':str(folder)}):
                with self.assertRaises(ValueError):inputs.archive_directory(root,'linux-host')
                (folder/metadata['11']['name']).write_bytes(b'bad')
                with self.assertRaises(ValueError):inputs.archive_directory(root,'linux-host')


if __name__=='__main__':unittest.main()
