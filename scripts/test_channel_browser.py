"""Launch arguments cannot silently select a different or temporary profile."""
from pathlib import Path
import tempfile
import unittest
import os
from contextlib import nullcontext
from unittest.mock import patch,Mock
from browser_lease import BrowserContext
from channel_browser import launch_arguments,launch,manage,main,owned_lease


class ChannelBrowserTests(unittest.TestCase):
    def test_expired_lock_deadline_does_not_sleep_or_retry(self):
        from test_browser_lease import Engine
        from browser_lease import BrowserLease
        with tempfile.TemporaryDirectory() as temporary:
            context=BrowserContext(Path(temporary),'fixture','ai-phone-bundle/desktop:sha-'+'a'*64);engine=Engine()
            with BrowserLease(context,engine),patch('channel_browser.time.monotonic',return_value=100),patch('channel_browser.time.sleep',side_effect=AssertionError('retried expired lock')):
                with self.assertRaises(BlockingIOError):
                    with owned_lease(context,engine,'a'*64):pass

    def test_management_can_wait_for_the_previous_kernel_owner_to_exit(self):
        import threading
        from test_browser_lease import Engine
        from browser_lease import BrowserLease
        with tempfile.TemporaryDirectory() as temporary:
            context=BrowserContext(Path(temporary),'fixture','ai-phone-bundle/desktop:sha-'+'a'*64);engine=Engine();first=BrowserLease(context,engine);first.__enter__()
            timer=threading.Timer(0.03,lambda:first.__exit__());timer.start()
            try:
                with owned_lease(context,engine,'a'*64,wait=0.5) as lease:lease.prepare_profile()
                self.assertTrue(context.marker.is_file())
            finally:
                timer.join()
                if first.file is not None:first.__exit__()

    def test_relay_failure_stops_only_its_owned_container_and_retains_the_profile(self):
        from test_browser_lease import Engine
        with tempfile.TemporaryDirectory() as temporary:
            context=BrowserContext(Path(temporary),'fixture','ai-phone-bundle/desktop:sha-'+'a'*64);engine=Engine()
            child=Mock();engine.attach=Mock(return_value=child)
            binary=patch('msvcrt.setmode') if os.name=='nt' else nullcontext()
            with binary,patch('channel_browser.os.fstat'),patch('channel_browser.run_relay',side_effect=OSError('pipe lost')):
                with self.assertRaisesRegex(OSError,'pipe lost'):launch(context,engine,'a'*64,['--user-data-dir='+str(context.profile),'--remote-debugging-pipe'])
            self.assertFalse(engine.containers);self.assertFalse(context.record.exists());self.assertTrue(context.marker.is_file())
            child.stdin.close.assert_called_once();child.wait.assert_called_once()
            self.assertEqual(manage(context,engine,'a'*64,'--stop'),{'stopped':True,'erased':False})
            self.assertTrue(context.marker.is_file())
            self.assertEqual(manage(context,engine,'a'*64,'--erase'),{'stopped':True,'erased':True});self.assertFalse(context.profile.exists())

    def test_superseded_stop_is_harmless_but_old_logout_cannot_erase_new_account(self):
        from test_browser_lease import Engine
        from browser_lease import BrowserLease,BrowserOwnerChanged
        with tempfile.TemporaryDirectory() as temporary:
            context=BrowserContext(Path(temporary),'fixture','ai-phone-bundle/desktop:sha-'+'a'*64);engine=Engine()
            with BrowserLease(context,engine,owner='b'*64) as lease:lease.create([])
            self.assertEqual(manage(context,engine,'a'*64,'--stop'),{'stopped':True,'superseded':True})
            with self.assertRaises(BrowserOwnerChanged):manage(context,engine,'a'*64,'--erase')
            self.assertTrue(engine.containers);manage(context,engine,'b'*64,'--erase')

    def test_bootstrap_checks_real_engine_without_creating_a_session(self):
        import io
        image='ai-phone-bundle/desktop:sha-'+'a'*64
        engine=Mock();engine.check.return_value='Chromium fixture'
        with tempfile.TemporaryDirectory() as temporary,patch('channel_browser.DockerEngine',return_value=engine),patch('sys.stdout',new_callable=io.StringIO) as output:
            main(['--check'],{'AI_PHONE_PROJECT_ROOT':temporary,'AI_PHONE_DESKTOP_IMAGE':image})
            import json
            self.assertTrue(json.loads(output.getvalue())['available']);self.assertFalse((Path(temporary)/'data').exists())
            with self.assertRaises(ValueError):main(['--stop'],{'AI_PHONE_PROJECT_ROOT':temporary,'AI_PHONE_DESKTOP_IMAGE':image,'AI_PHONE_BROWSER_SESSION':'fixture'})

    def test_launch_requires_the_explicit_persistent_profile_and_pipe(self):
        with tempfile.TemporaryDirectory() as temporary:
            context=BrowserContext(Path(temporary),'fixture','ai-phone-bundle/desktop:sha-'+'a'*64)
            profile='--user-data-dir='+str(context.profile)
            self.assertEqual(launch_arguments(context,[profile,'--remote-debugging-pipe','--user-agent=fixture']),['--user-agent=fixture'])
            for args in ([profile],['--remote-debugging-pipe'],[profile,profile,'--remote-debugging-pipe'],['--user-data-dir='+str(Path(temporary)/'else'),'--remote-debugging-pipe'],[profile,'--remote-debugging-pipe','--remote-debugging-port=9222'],[profile,'--remote-debugging-pipe','http://else']):
                with self.assertRaises(ValueError):launch_arguments(context,args)
            base=[profile,'--remote-debugging-pipe'];valid=[*base,*(['--flag']*62)];self.assertEqual(len(launch_arguments(context,valid)),62)
            with self.assertRaises(ValueError):launch_arguments(context,[*valid,'--extra'])
            exact=[*base,'--'+('x'*(16384-sum(map(len,base))-2))];self.assertEqual(launch_arguments(context,exact),exact[2:])
            with self.assertRaises(ValueError):launch_arguments(context,[*base,exact[2]+'x'])


if __name__=='__main__':unittest.main()
