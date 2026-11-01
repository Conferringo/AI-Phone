"""Installer compression must preserve exact TAR bytes, including script/mode headers."""
import gzip
import hashlib
import io
import lzma
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import debian_compression as compression


def header(name, size):
    return (f'{name+"/":<16}{"1700000000":<12}{"0":<6}{"0":<6}{"100644":<8}{size:<10}`\n').encode('ascii')


def archive(path, members):
    with path.open('wb') as output:
        output.write(b'!<arch>\n')
        for name, data in members:
            output.write(header(name, len(data)));output.write(data)
            if len(data)%2:output.write(b'\n')


def members(path):
    rows=[]
    with path.open('rb') as source:
        assert source.read(8)==b'!<arch>\n'
        while row:=source.read(60):
            size=int(row[48:58]);data=source.read(size)
            rows.append((row[:16].decode().strip().rstrip('/'), data, row[16:48]))
            if size%2:source.read(1)
    return rows


def tar_bytes(name, content, mode):
    output=io.BytesIO()
    with tarfile.open(fileobj=output, mode='w',format=tarfile.USTAR_FORMAT) as target:
        info=tarfile.TarInfo(name);info.size=len(content);info.mode=mode;info.uid=0;info.gid=0;info.mtime=1700000000
        target.addfile(info,io.BytesIO(content))
    return output.getvalue()


class DebianCompressionTests(unittest.TestCase):
    def fixture(self, root):
        output=io.BytesIO()
        with tarfile.open(fileobj=output,mode='w',format=tarfile.USTAR_FORMAT) as target:
            rows=[('./postinst',b'#!/bin/sh\nset -eu\nexit 0\n',0o755),('./control',b'Package: ai-phone-compression-fixture\nVersion: 1\nArchitecture: all\nMaintainer: AI Phone contributors <fixture@localhost>\nDescription: Compression qualification fixture\n',0o644)]
            for name,content,mode in rows:
                info=tarfile.TarInfo(name);info.size=len(content);info.mode=mode;info.uid=0;info.gid=0;info.mtime=1700000000
                target.addfile(info,io.BytesIO(content))
        control=output.getvalue()
        data=tar_bytes('./usr/lib/ai-phone/host/tools',bytes(range(256))*16384,0o640)
        source=root/'original.deb'
        archive(source,[('debian-binary',b'2.0\n'),('control.tar.gz',gzip.compress(control)),('data.tar.gz',gzip.compress(data))])
        return source,control,data

    def test_recompresses_the_same_installation_and_retains_ar_metadata(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source,control,data=self.fixture(root);before=source.read_bytes();output=root/'complete.deb'
            receipt=compression.recompress(source,output)
            rows=members(output)
            self.assertLess(output.stat().st_size,source.stat().st_size)
            self.assertEqual(source.read_bytes(),before)
            self.assertEqual([row[0] for row in rows],['debian-binary','control.tar.xz','data.tar.xz'])
            self.assertEqual(rows[0][1],b'2.0\n')
            for row,raw in zip(rows[1:],(control,data)):
                self.assertEqual(lzma.decompress(row[1]),raw)
                self.assertEqual(receipt['members'][row[0]]['raw_sha256'],hashlib.sha256(raw).hexdigest())
            self.assertEqual([r[2] for r in rows],[r[2] for r in members(source)])
            self.assertEqual(receipt['compression'],'xz')

    def test_failed_input_preserves_an_existing_installer_and_removes_temporary_files(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source,_,_=self.fixture(root);output=root/'complete.deb';output.write_bytes(b'previous complete installer')
            valid=source.read_bytes()
            bad_size=valid[:56]+b'invalid   '+valid[66:]
            for invalid in (valid[:-3],valid+b'extra',valid.replace(b'data.tar.gz/',b'evil.tar.gz/',1),valid.replace(b'`\n',b'!!',1),bad_size):
                source.write_bytes(invalid)
                with self.assertRaises((ValueError,EOFError,OSError)):compression.recompress(source,output)
                self.assertEqual(output.read_bytes(),b'previous complete installer')
                self.assertEqual(sorted(p.name for p in root.iterdir()),['complete.deb','original.deb'])

    def test_checksum_failure_cannot_publish_partial_output(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source,_,data=self.fixture(root);output=root/'complete.deb'
            broken=bytearray(gzip.compress(data));broken[-8]^=1
            archive(source,[('debian-binary',b'2.0\n'),('control.tar.gz',gzip.compress(b'control')),('data.tar.gz',broken)])
            with self.assertRaises((ValueError,EOFError,OSError)):compression.recompress(source,output)
            self.assertFalse(output.exists())

    def test_incompressible_small_archive_uses_the_original_without_growth(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source=root/'tiny.deb';output=root/'complete.deb'
            archive(source,[('debian-binary',b'2.0\n'),('control.tar.gz',gzip.compress(b'c')),('data.tar.gz',gzip.compress(b'd'))])
            receipt=compression.recompress(source,output)
            self.assertEqual(output.read_bytes(),source.read_bytes())
            self.assertEqual(receipt['compression'],'gzip')
            self.assertEqual(set(receipt['members']),{'control.tar.gz','data.tar.gz'})

    def test_package_only_path_inspects_the_repacked_installer(self):
        import build_client
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);original,control,data=self.fixture(root)
            (root/'config').mkdir();(root/'config/release-policy.json').write_text('{"version":"fixture"}')
            package=root/'apps/client/src-tauri/target/release/bundle/deb/AI Phone_fixture_amd64.deb'
            package.parent.mkdir(parents=True);package.write_bytes(original.read_bytes())
            destination=root/'dist/AI-Phone-fixture-linux-amd64.deb'
            with patch.object(build_client,'ROOT',root),patch.object(build_client.platform,'system',return_value='Linux'),patch.object(build_client.platform,'machine',return_value='x86_64'),patch.object(build_client,'inspect_deb',return_value=[]) as inspect,patch.object(build_client,'record',return_value={'server_included':True,'complete_device_bundle':True}):
                build_client.build_other('linux',package_only=True)
            inspect.assert_called_once_with(destination)
            self.assertEqual([lzma.decompress(row[1]) for row in members(destination)[1:]],[control,data])
            self.assertEqual(package.read_bytes(),original.read_bytes())

    def test_repackaging_can_atomically_replace_the_input(self):
        with tempfile.TemporaryDirectory() as folder:
            source,control,data=self.fixture(Path(folder))
            compression.recompress(source,source)
            self.assertEqual([lzma.decompress(row[1]) for row in members(source)[1:]],[control,data])

    def test_accepts_the_common_ar_headers_written_by_tauri(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source,control,data=self.fixture(root)
            encoded=source.read_bytes()
            for name in compression.MEMBERS:
                encoded=encoded.replace((name+'/').encode().ljust(16),name.encode().ljust(16),1)
            source.write_bytes(encoded);destination=root/'nested/package/complete.deb'
            receipt=compression.recompress(source,destination)
            self.assertEqual([lzma.decompress(row[1]) for row in members(destination)[1:]],[control,data])
            self.assertEqual([row['raw_bytes'] for row in receipt['members'].values()],[len(control),len(data)])

    def test_equal_size_representations_retain_the_original_package(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);source=root/'original.deb';candidate=root/'xz.deb';destination=root/'complete.deb'
            compressed=gzip.compress(b'd')
            archive(source,[('debian-binary',b'2.0\n'),('control.tar.gz',gzip.compress(b'c')),('data.tar.gz',compressed)])
            archive(candidate,[('debian-binary',b'2.0\n'),('control.tar.xz',lzma.compress(b'c')),('data.tar.xz',lzma.compress(b'd'))])
            delta=candidate.stat().st_size-source.stat().st_size
            self.assertGreater(delta,2)
            padded=compressed[:3]+bytes([compressed[3]|4])+compressed[4:10]+(delta-2).to_bytes(2,'little')+bytes(delta-2)+compressed[10:]
            archive(source,[('debian-binary',b'2.0\n'),('control.tar.gz',gzip.compress(b'c')),('data.tar.gz',padded)])
            self.assertEqual(source.stat().st_size,candidate.stat().st_size)
            receipt=compression.recompress(source,destination)
            self.assertEqual(receipt['compression'],'gzip')
            self.assertEqual(source.read_bytes(),destination.read_bytes())


if __name__=='__main__':unittest.main()
