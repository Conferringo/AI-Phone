import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from artifact_io import digest
import package_delivery as delivery


class CandidateRetentionTests(unittest.TestCase):
    def test_linux_collection_contains_only_the_complete_deb_and_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);legacy=self.fixture(root)
            deb={key:legacy[key] for key in ('artifact','package_delivery')}
            report={key:value for key,value in legacy.items() if key not in ('artifact','package_delivery')}
            report.update({'format':2,'installers':[deb]})
            plan=delivery.upload_plan(root,report,'a'*40)
            self.assertEqual(len(plan['files']),3)
            for entries in ([],[deb,deb]):
                with self.assertRaisesRegex(ValueError,'formats'):delivery.upload_plan(root,{**report,'installers':entries},'a'*40)

    def fixture(self,root):
        (root/'dist').mkdir();(root/'SOURCE-MANIFEST.json').write_bytes(b'reviewed source fixture')
        artifact=root/'dist/AI-Phone-fixture.deb';artifact.write_bytes(b'installer fixture')
        metadata={'sha256':digest(artifact),'complete_device_bundle':True,'source_commit':'a'*40,
                  'local_bootstrap':{'user_state_bundled':False}}
        artifact.with_suffix('.deb.manifest.json').write_text(json.dumps(metadata))
        artifact.with_suffix('.deb.sha256').write_text(digest(artifact)+'  '+artifact.name+'\n')
        record={'file':artifact.name,'size':artifact.stat().st_size,'sha256':digest(artifact),'delivery':{'complete_device_bundle':True}}
        report={'format':1,'source_commit':'a'*40,'source_manifest_sha256':digest(root/'SOURCE-MANIFEST.json'),
                'host':'linux-x64','binary_publication':False,'artifact':record,'package_delivery':{'format':'single'}}
        return report

    def test_plan_contains_only_complete_installer_and_its_reviewed_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);report=self.fixture(root)
            (root/'dist/private-evidence.txt').write_bytes(b'excluded fixture')
            plan=delivery.upload_plan(root,report,'a'*40)
            self.assertEqual({r['name'] for r in plan['files']},{'AI-Phone-fixture.deb','AI-Phone-fixture.deb.sha256','AI-Phone-fixture.deb.manifest.json'})
            self.assertFalse(plan['binary_publication'])
            for row in plan['files']:self.assertEqual(row['sha256'],digest(root/'dist'/row['path']))

    def test_wrong_source_incomplete_bundle_changed_file_and_escaping_path_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);report=self.fixture(root)
            for changed in ({**report,'source_commit':'b'*40},{**report,'binary_publication':True},
                            {**report,'source_manifest_sha256':'0'*64},
                            {**report,'artifact':{**report['artifact'],'delivery':{'complete_device_bundle':False}}}):
                with self.assertRaises(ValueError):delivery.upload_plan(root,changed,'a'*40)
            (root/'dist/AI-Phone-fixture.deb').write_bytes(b'changed')
            with self.assertRaises(ValueError):delivery.upload_plan(root,report,'a'*40)
            report['artifact']['file']='../outside'
            with self.assertRaises(ValueError):delivery.upload_plan(root,report,'a'*40)

    def test_remote_digest_size_state_and_unpublished_source_must_all_match(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);plan=delivery.upload_plan(root,self.fixture(root),'a'*40)
            assets=[{'name':r['name'],'size':r['size'],'digest':'sha256:'+r['sha256'],'state':'uploaded'} for r in plan['files']]
            release={'draft':True,'published_at':None,'target_commitish':'a'*40,'assets':assets}
            self.assertTrue(delivery.verify_uploaded(plan,release))
            for changed in ({**release,'draft':False},{**release,'published_at':'published'},
                            {**release,'target_commitish':'b'*40},{**release,'assets':assets[:-1]}):
                with self.assertRaises(ValueError):delivery.verify_uploaded(plan,changed)
            for change in ({'digest':'sha256:'+'0'*64},{'size':0},{'state':'starter'}):
                changed={**release,'assets':[{**assets[0],**change},*assets[1:]]}
                with self.assertRaises(ValueError):delivery.verify_uploaded(plan,changed)

    def test_volume_plan_uses_the_verified_manifest_and_never_uploads_the_large_original(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);report=self.fixture(root);artifact=report['artifact'];folder=root/'dist/volumes';folder.mkdir()
            body=(root/'dist'/artifact['file']).read_bytes();parts=[]
            for index,start in enumerate(range(0,len(body),8)):
                path=folder/(artifact['file']+'.part-'+str(index+1).zfill(4));path.write_bytes(body[start:start+8])
                parts.append({'name':path.name,'size':path.stat().st_size,'sha256':digest(path)})
            manifest=folder/(artifact['file']+'.volumes.json');manifest.write_text(json.dumps({'parts':parts}))
            report['package_delivery']={'format':'volumes','manifest':manifest.relative_to(root/'dist').as_posix(),'manifest_sha256':digest(manifest)}
            verified={'verified':True,'sha256':artifact['sha256'],'size':artifact['size']}
            with patch.object(delivery,'native',return_value=verified) as native:
                plan=delivery.upload_plan(root,report,'a'*40)
                native.assert_called_once_with(root,'verify',manifest,digest(manifest))
            names={r['name'] for r in plan['files']};self.assertNotIn(artifact['file'],names)
            self.assertTrue({p['name'] for p in parts}<=names)
            with patch.object(delivery,'native',return_value={**verified,'verified':False}),self.assertRaises(ValueError):delivery.upload_plan(root,report,'a'*40)
            # The native verifier and uploader are different processes. A changed
            # volume must not acquire a new upload identity under the old manifest.
            def changed_after_verification(*args):
                first=folder/parts[0]['name']
                data=first.read_bytes();first.write_bytes(bytes([data[0]^1])+data[1:])
                return verified
            with patch.object(delivery,'native',side_effect=changed_after_verification),self.assertRaisesRegex(ValueError,'volume changed'):
                delivery.upload_plan(root,report,'a'*40)


if __name__=='__main__':unittest.main()
