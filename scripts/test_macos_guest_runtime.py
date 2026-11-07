"""Static guest admission fixtures; no compiler, engine or SDK is executed."""
import json
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch
from artifact_io import digest
import macos_guest_runtime as guest
import linux_client_factory as images
import macos_client_factory as mac


def elf(machine=183,kind=1):
    header=bytearray(64);header[:6]=b'\x7fELF\x02\x01';struct.pack_into('<H',header,18,machine)
    struct.pack_into('<Q',header,32,64);struct.pack_into('<HH',header,54,56,1)
    program=bytearray(56);struct.pack_into('<I',program,0,kind)
    return bytes(header+program)


class MacGuestRuntimeTests(unittest.TestCase):
    def test_guest_identity_includes_the_protocol_and_api_it_compiles(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);(root/'Cargo.toml').write_text('fixture source')
            for name in ('crates/api/src/lib.rs','packages/protocol/proto/ai_phone/v1/location.proto'):
                source=root/name;source.parent.mkdir(parents=True,exist_ok=True);source.write_text('first source')
                before=guest.source_identity(root);source.write_text('changed compiled dependency')
                self.assertNotEqual(guest.source_identity(root),before,name)

    def test_precompiled_context_is_materialized_inside_the_shared_factory_tree(self):
        with tempfile.TemporaryDirectory(prefix='shared factory ') as shared,tempfile.TemporaryDirectory(prefix='unmounted compiler ') as outside:
            root=Path(shared).resolve();(root/'Cargo.toml').write_text('fixture source')
            source=Path(outside)/'context';binary=source/guest.BINARY;binary.parent.mkdir(parents=True);binary.write_bytes(elf())
            manifest={'format':1,'target':guest.TARGET,'source_sha256':guest.source_identity(root),'sha256':digest(binary)}
            (source/'manifest.json').write_text(json.dumps({**manifest,'unrelated':'private-fixture'}))
            (source/'unrelated.env').write_text('private-fixture')
            with patch.dict(guest.os.environ,{'AI_PHONE_FACTORY_GUEST_RUNTIME':str(source)}),patch.object(mac,'build_guest') as compile:
                result=mac.prepare_guest(root,root/'.tools/complete-macos')
            self.assertTrue(result.is_relative_to(root),'the remote Docker CLI can read only the factory mounts')
            self.assertNotEqual(result,source)
            self.assertEqual(guest.verify(root,result,'arm64'),manifest)
            self.assertEqual((result/guest.BINARY).read_bytes(),binary.read_bytes())
            self.assertEqual((result/guest.BINARY).stat().st_mode&0o777,0o755)
            self.assertEqual({p.relative_to(result).as_posix() for p in result.rglob('*') if p.is_file()},{guest.BINARY,'manifest.json'})
            self.assertEqual((source/'unrelated.env').read_text(),'private-fixture')
            compile.assert_not_called()

    def test_rejects_wrong_architecture_dynamic_interpreters_and_bad_headers(self):
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'bridge'
            for data in (elf(machine=62),elf(kind=3),elf()[:70],b'not an ELF'):
                path.write_bytes(data)
                with self.assertRaises(ValueError):guest.static_arm_elf(path)
            path.write_bytes(elf());guest.static_arm_elf(path)

    def test_component_binds_compiled_bytes_to_the_current_guest_sources(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);(root/'Cargo.toml').write_text('fixture source')
            folder=root/'component';binary=folder/guest.BINARY;binary.parent.mkdir(parents=True);binary.write_bytes(elf())
            manifest={'target':guest.TARGET,'source_sha256':guest.source_identity(root),'sha256':digest(binary)}
            (folder/'manifest.json').write_text(json.dumps(manifest))
            self.assertEqual(guest.verify(root,folder,'arm64'),manifest)
            with self.assertRaises(ValueError):guest.verify(root,folder,'amd64')
            binary.write_bytes(elf()+b'changed')
            with self.assertRaisesRegex(ValueError,'changed'):guest.verify(root,folder,'arm64')
            binary.write_bytes(elf());(root/'Cargo.toml').write_text('new source')
            with self.assertRaisesRegex(ValueError,'source'):guest.verify(root,folder,'arm64')
            (root/'Cargo.toml').write_text('fixture source')
            source=root/'crates/guest-runtime/src/main.rs';source.parent.mkdir(parents=True);source.write_text('changed guest code')
            with self.assertRaisesRegex(ValueError,'source'):guest.verify(root,folder,'arm64')

    def test_existing_work_and_missing_toolchain_cannot_start_a_compiler(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);work=root/'work';work.mkdir()
            with patch.object(guest.subprocess,'run') as run:
                with self.assertRaisesRegex(ValueError,'fresh'):guest.build(root,work)
                with patch.object(guest.subprocess,'check_output',return_value='rustc 1.95.0'):
                    with self.assertRaisesRegex(ValueError,'1.96.0'):guest.build(root,root/'new')
                run.assert_not_called()

    def test_native_compiler_outputs_a_source_bound_static_context_with_literal_paths(self):
        with tempfile.TemporaryDirectory(prefix='guest compiler path ') as temporary:
            root=Path(temporary).resolve();(root/'Cargo.toml').write_text('fixture source');sysroot=root/'sysroot'
            linker=sysroot/'lib/rustlib/aarch64-apple-darwin/bin/rust-lld';linker.parent.mkdir(parents=True);linker.write_bytes(b'fixture linker')
            (sysroot/'lib/rustlib'/guest.TARGET/'lib').mkdir(parents=True)
            work=root/'compiler'
            def compile_fixture(argv,*,cwd,env,check):
                self.assertEqual(cwd,root);self.assertTrue(check)
                self.assertEqual(argv,['cargo','build','--release','--locked','--package','ai-phone-guest-runtime','--bin','desktop-bridge','--target',guest.TARGET])
                self.assertEqual(env['CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER'],str(linker))
                self.assertIn('-Clinker-flavor=ld.lld',env['CARGO_ENCODED_RUSTFLAGS'].split('\x1f'))
                binary=Path(env['CARGO_TARGET_DIR'])/guest.TARGET/'release/desktop-bridge'
                binary.parent.mkdir(parents=True);binary.write_bytes(elf())
            with patch.object(guest.subprocess,'check_output',side_effect=['rustc 1.96.0 (fixture)',str(sysroot)]),patch.object(guest.subprocess,'run',side_effect=compile_fixture):
                folder=guest.build(root,work)
            self.assertEqual(guest.verify(root,folder,'arm64')['sha256'],digest(folder/guest.BINARY))
            self.assertEqual((folder/guest.BINARY).stat().st_mode&0o777,0o755)
            for role in ('workspace','desktop'):
                with patch.object(images,'docker_image',return_value=None),patch.object(images,'run') as run:
                    images.build_image(root,role,'fixture-'+role,workspace='fixture-workspace',clash=root/'clash',architecture='arm64',guest_runtime=folder)
                command=run.call_args.args[0]
                self.assertIn('GUEST_RUNTIME_SOURCE=guest_runtime',command)
                self.assertIn('guest_runtime='+str(folder.resolve()),command)
                self.assertEqual(command[command.index('--platform')+1],'linux/arm64')
                self.assertEqual(command[-1],root)
                self.assertEqual(command[command.index('--file')+1],str(root/'infra'/role/'Dockerfile'))

    def test_precompiled_component_is_verified_before_it_can_replace_compilation(self):
        with patch.dict(guest.os.environ,{'AI_PHONE_FACTORY_GUEST_RUNTIME':'fixture context'}),patch.object(guest,'verify',side_effect=ValueError('Guest component source mismatch')) as verify,patch.object(mac,'build_guest') as compile:
            with self.assertRaisesRegex(ValueError,'source'):mac.prepare_guest(Path('/fixture'),Path('/cache'))
            verify.assert_called_once_with(Path('/fixture'),Path('fixture context'),'arm64')
            compile.assert_not_called()


if __name__=='__main__':unittest.main()
