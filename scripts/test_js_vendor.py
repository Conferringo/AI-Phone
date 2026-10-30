"""Only an exact published resource may carry upstream build-directory strings."""
import hashlib
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

class VendorOriginTests(unittest.TestCase):
    def test_exact_approved_bytes_do_not_waive_secret_or_modified_body_checks(self):
        from js_vendor import dependency_bytes
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();package=root/'node_modules/vendor';package.mkdir(parents=True);source=package/'library.node';source.write_bytes(b'upstream public debug paths')
            meta={'name':'vendor','version':'1.0'};approval={('vendor','1.0','library.node'):hashlib.sha256(source.read_bytes()).hexdigest()}
            with patch('prebuilt_web.public_runtime_file',side_effect=ValueError('private path')):
                self.assertEqual(dependency_bytes(root,package,meta,source,'runtime/js-services/node_modules/vendor/library.node',approval),b'upstream public debug paths')
                source.write_bytes(b'changed')
                with self.assertRaises(ValueError):dependency_bytes(root,package,meta,source,'runtime/js-services/node_modules/vendor/library.node',approval)
                secret=b'sk-'+b'a'*32;source.write_bytes(secret);approval[('vendor','1.0','library.node')]=hashlib.sha256(secret).hexdigest()
                with self.assertRaises(ValueError):dependency_bytes(root,package,meta,source,'runtime/js-services/node_modules/vendor/library.node',approval)


class VendorManifestTests(unittest.TestCase):
    def test_missing_policy_and_complete_exact_origin(self):
        import json
        from js_vendor import read_approvals
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.assertEqual(read_approvals(root),{})
            (root/'config').mkdir();row=self.record();(root/'config/js-runtime-vendor.json').write_text(json.dumps({'format':1,'assets':[row]}))
            self.assertEqual(read_approvals(root),{('vendor','1.0','lib/addon.node'):'a'*64})

    def record(self):
        return {'package':'vendor','version':'1.0','file':'lib/addon.node','sha256':'a'*64,'archive_url':'https://registry.npmjs.org/vendor/-/vendor-1.0.tgz','archive_integrity':'sha512-'+'A'*86+'==','archive_sha256':'b'*64}

    def test_malformed_origin_and_policy_never_approve(self):
        import json
        from js_vendor import read_approvals
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);(root/'config').mkdir();path=root/'config/js-runtime-vendor.json';row=self.record()
            variants=[None,{'format':True,'assets':[]},{'format':2,'assets':[]},{'format':1,'assets':{}},{'format':1,'assets':[row,row]},{'format':1,'assets':[None]}]
            for field,value in [('package',''),('file','../escape'),('sha256','wrong'),('archive_sha256','wrong'),('archive_integrity','sha512-invalid'),('archive_url','https://other.invalid/archive')]:
                variants.append({'format':1,'assets':[{**row,field:value}]})
            for value in variants:
                with self.subTest(value=value):
                    path.write_text(json.dumps(value))
                    with self.assertRaises(ValueError):read_approvals(root)
            path.write_bytes(b'x'*65537)
            with self.assertRaises(ValueError):read_approvals(root)
            path.unlink();outside=root/'other';outside.write_text('{"format":1,"assets":[]}');path.symlink_to(outside)
            with self.assertRaises(ValueError):read_approvals(root)

    def test_exact_policy_limits_and_utf16_credentials(self):
        import json
        from js_vendor import read_approvals,dependency_bytes
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);(root/'config').mkdir();rows=[{**self.record(),'file':f'lib/addon-{i}.node'} for i in range(32)]
            body=json.dumps({'format':1,'assets':rows});path=root/'config/js-runtime-vendor.json';path.write_text(body+' '*(65536-len(body)));self.assertEqual(len(read_approvals(root)),32)
            path.write_text(json.dumps({'format':1,'assets':rows+[self.record()]}))
            with self.assertRaises(ValueError):read_approvals(root)
            package=root/'package';package.mkdir();source=package/'addon.node';data=('sk-'+'a'*32).encode('utf-16le');source.write_bytes(data)
            with self.assertRaises(ValueError):dependency_bytes(root,package,{'name':'vendor','version':'1'},source,'runtime/js-services/package/addon.node',{('vendor','1','addon.node'):hashlib.sha256(data).hexdigest()})


if __name__=='__main__':unittest.main()
