"""A sealed XZ capsule imports through raw TAR without requiring engine-side xz."""
import copy
import gzip
import hashlib
import io
import json
import lzma
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import container_images as images
import test_container_images as legacy


def capsule(root):
    old,manifest=legacy.ContainerImageTests().fixture(root)
    with images.stage(old).archive() as source:raw=gzip.decompress(source.read())
    value=copy.deepcopy(manifest);packed=lzma.compress(raw)
    value.update(format=2,archive={'path':'images.tar.xz','codec':'xz','size':len(packed),'sha256':hashlib.sha256(packed).hexdigest()})
    output=root/'new.payload'
    with tarfile.open(output,'w') as archive:
        for name,data in [('IMAGES-MANIFEST.json',json.dumps(value).encode()),('images.tar.xz',packed)]:
            entry=tarfile.TarInfo(name);entry.size=len(data);archive.addfile(entry,io.BytesIO(data))
    return images.stage(output),raw


class ImageImportStreamTests(unittest.TestCase):
    def test_raw_budget_failure_cannot_publish_a_selection(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);prepared,raw=capsule(root)
            def docker(args,source=None):
                if args[0]=='info':return 'arm64'
                if args[:2]==['image','inspect']:return None
                if args[:2]==['image','load']:source.read()
            with patch.object(images,'MAX_RAW',len(raw)-1),self.assertRaisesRegex(ValueError,'budget'):
                images.activate(root,prepared,docker)
            self.assertFalse((root/'data/deployment/image-runtime.json').exists())

    def test_xz_import_stream_is_the_exact_raw_tar_without_a_second_disk_copy(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);prepared,raw=capsule(root);ready={};loads=[]
            def docker(args,source=None):
                if args[0]=='info':return 'arm64'
                if args[:2]==['image','inspect']:return json.dumps([ready[args[2]]]) if args[2] in ready else None
                if args[:2]==['image','load']:
                    body=source.read();loads.append(body)
                    self.assertEqual(body,raw,'The portable engine accepts raw TAR, not compressed XZ')
                    with tarfile.open(fileobj=io.BytesIO(body),mode='r:') as archive:self.assertIn('manifest.json',archive.getnames())
                    for row in prepared.manifest['images'].values():ready[row['ref']]={'Id':row['config_digest'],'Architecture':'arm64','Os':'linux'}
                    return ''
                self.fail(args)
            result=images.activate(root,prepared,docker)
            self.assertEqual(loads,[raw]);self.assertEqual(result['AI_PHONE_IMAGE_PULL_POLICY'],'never')
            self.assertEqual(sorted(p.name for p in root.iterdir()),['data','images.payload','new.payload'])

    def test_interrupted_raw_import_publishes_nothing_and_observes_before_any_retry(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);prepared,raw=capsule(root);ready={};loads=[]
            def docker(args,source=None):
                if args[0]=='info':return 'arm64'
                if args[:2]==['image','inspect']:return json.dumps([ready[args[2]]]) if args[2] in ready else None
                if args[:2]==['image','load']:
                    loads.append(source.read())
                    for row in prepared.manifest['images'].values():ready[row['ref']]={'Id':row['config_digest'],'Architecture':'arm64','Os':'linux'}
                    raise OSError('fixture import result unknown')
                self.fail(args)
            with self.assertRaisesRegex(OSError,'unknown'):images.activate(root,prepared,docker)
            self.assertFalse((root/'data/deployment/image-runtime.json').exists())
            images.activate(root,prepared,docker)
            self.assertEqual(loads,[raw]);self.assertTrue((root/'data/deployment/image-runtime.json').is_file())


if __name__=='__main__':unittest.main()
