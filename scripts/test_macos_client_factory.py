"""Full Mac assembly contracts; these fixtures run no SDK, engine or build tool."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from artifact_io import digest
import macos_client_factory as mac
import linux_client_factory as images

COMPONENTS=['web','native-services','phone-apps','container-host','js-services',
            'guest-apps','android-runtime','container-images','installer-tools']


class MacClientFactoryTests(unittest.TestCase):
    def test_complete_inventory_requires_every_device_resource(self):
        self.assertTrue(mac.checked_delivery(COMPONENTS)['complete_device_bundle'])
        for missing in COMPONENTS:
            with self.subTest(missing=missing),self.assertRaises(ValueError):
                mac.checked_delivery([name for name in COMPONENTS if name!=missing])

    def test_compiled_dmg_requires_full_inventory_and_matching_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);(root/'config').mkdir();(root/'dist').mkdir()
            (root/'config/release-policy.json').write_text('{"version":"fixture"}')
            artifact=root/'dist/AI-Phone-fixture-macos-arm64.dmg';artifact.write_bytes(b'fixture artifact')
            path=artifact.with_suffix('.dmg.manifest.json')
            receipt={'complete_device_bundle':True,'sha256':digest(artifact),'prebuilt_components':COMPONENTS}
            env={'ANDROID_HOME':'fixture-sdk','JAVA_HOME':'fixture-java'}
            with patch.object(mac,'run') as run,patch.object(mac,'run_checks') as checks,patch('prepare_client_host.payload_components',return_value=COMPONENTS):
                path.write_text(json.dumps(receipt))
                value=mac.compile_client(root,root/'android',root/'images',root/'tools',env)
                self.assertTrue(value['delivery']['complete_device_bundle'])
                checks.assert_called_once_with(root,env=env)
                command=run.call_args.args[0]
                for flag,name in [('--android-runtime','android'),('--images-runtime','images'),('--installer-tools','tools')]:
                    self.assertEqual(command[command.index(flag)+1],root/name)
                for flag in ('--with-web-runtime','--with-service-runtime','--with-phone-apps','--with-container-host','--with-js-services','--with-guest-apps'):
                    self.assertIn(flag,command)
                self.assertEqual(run.call_args.kwargs['env'],env)
                for changed in ({**receipt,'complete_device_bundle':False},{**receipt,'prebuilt_components':COMPONENTS[:-1]},{**receipt,'sha256':'0'*64}):
                    path.write_text(json.dumps(changed))
                    with self.assertRaises(ValueError):mac.compile_client(root,root/'android',root/'images',root/'tools',env)

    def test_arm_image_builder_uses_arm_platform_and_arm_clash_package(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);(root/'config').mkdir();cache=root/'cache';cache.mkdir()
            arm={'file':'arm.deb','url':'fixture-arm'};intel={'file':'intel.deb','url':'fixture-intel'}
            (root/'config/clash-apps.json').write_text(json.dumps({'linux':arm,'alternatives':{'linux-amd64':intel}}))
            (root/'compose.yaml').write_text('\n'.join('${AI_PHONE_'+role.upper()+'_IMAGE:-'+role+':fixture@sha256:'+'a'*64+'}' for role in ('postgres','redis')))
            with patch.object(images,'fetch') as fetch,patch.object(images,'docker_image',return_value=None),patch.object(images,'run') as run,patch('build_container_images.build',return_value={'architecture':'arm64'}) as seal:
                output,value=images.prepare_images(root,cache,'b'*64,architecture='arm64')
                self.assertEqual(output,cache/'images.payload');self.assertEqual(value['architecture'],'arm64')
                fetch.assert_called_once_with(cache/'clash','arm.deb',arm)
                self.assertEqual(len(run.call_args_list),4)
                for call in run.call_args_list:
                    command=call.args[0];self.assertEqual(command[command.index('--platform')+1],'linux/arm64')
                self.assertEqual(set(seal.call_args.args[0]),{'workspace','desktop','postgres','redis'})

    def test_existing_x64_builder_keeps_its_platform_and_invalid_arch_never_dispatches(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            with patch.object(images,'docker_image',return_value=None),patch.object(images,'run') as run:
                images.build_image(root,'workspace','fixture/workspace:build')
                command=run.call_args.args[0];self.assertEqual(command[command.index('--platform')+1],'linux/amd64')
                run.reset_mock()
                with self.assertRaises(ValueError):images.build_image(root,'workspace','fixture',architecture='x86')
                run.assert_not_called()

    def test_foreign_host_is_rejected_before_tools_or_engine_access(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);kept=root/'keep';kept.write_bytes(b'user fixture')
            with patch.object(mac,'host_key',return_value='linux-x64'),patch.object(mac,'compiler_environment') as compiler,patch.object(mac,'check_docker') as docker:
                with self.assertRaises(ValueError):mac.build(root,root/'sdk')
                compiler.assert_not_called();docker.assert_not_called()
            self.assertEqual(kept.read_bytes(),b'user fixture')

    def test_full_orchestrator_binds_arm_images_and_preserves_existing_outputs(self):
        from contextlib import ExitStack
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();(root/'dist').mkdir();manifest=root/'SOURCE-MANIFEST.json';manifest.write_bytes(b'fixture source')
            expected=digest(manifest);trace=[]
            with ExitStack() as stack:
                def mocked(name,value):
                    return stack.enter_context(patch.object(mac,name,side_effect=lambda *args,**kwargs:(trace.append(name),value)[1]))
                stack.enter_context(patch.object(mac,'host_key',return_value='macos-arm64'))
                stack.enter_context(patch('android_factory_stock.reserve'))
                stack.enter_context(patch('container_images.daemon_architecture',return_value='arm64'))
                stack.enter_context(patch('check_source.check'))
                delivery=stack.enter_context(patch('package_delivery.prepare',return_value={'format':'fixture'}))
                mocked('compiler_environment',{'JAVA_HOME':'fixture'});mocked('check_docker',None)
                mocked('build_guest',root/'guest')
                mocked('prepare_tools',(root/'tools',root/'python',{}));mocked('prepare_host',{})
                mocked('prepare_android',(root/'android',{}));mocked('prepare_apps',{'phone':{'source_sha256':'c'*64}})
                prepare=mocked('prepare_images',(root/'images',{'bytes':123}));mocked('compile_client',{'delivery':mac.checked_delivery(COMPONENTS)})
                result=mac.build(root,root/'compiler-sdk')
                delivery.assert_called_once_with(root,result['artifact'])
                self.assertEqual(result['package_delivery'],{'format':'fixture'})
                prepare.assert_called_once_with(root,root/'.tools/complete-macos',expected,architecture='arm64',guest_runtime=root/'guest')
                self.assertEqual(trace,['compiler_environment','check_docker','build_guest','prepare_tools','prepare_host','prepare_android','prepare_apps','prepare_images','compile_client'])
                self.assertEqual(result['source_manifest_sha256'],expected)
                self.assertFalse(result['binary_publication'])
                self.assertTrue((root/'dist/macos-full-factory-report.json').is_file())
                with self.assertRaisesRegex(ValueError,'fresh'):mac.build(root,root/'compiler-sdk')
