import unittest
from pathlib import Path
from host_platform import host_policy
import release

ROOT=Path(__file__).resolve().parent.parent

class HostPlatformTests(unittest.TestCase):
    def test_linux_uses_an_x64_phone_and_its_own_verified_tools(self):
        policy,host=host_policy(ROOT,'Linux','x86_64')
        self.assertEqual(host['phone_abi'],'x86_64')
        self.assertEqual(host['phone_template'],'android35-x86_64')
        self.assertIn(host['mise'],policy['mise'])
        self.assertIn(host['lima'],policy['lima_hosts'])
        for group,key in [('mise',host['mise']),('lima_hosts',host['lima'])]:
            self.assertEqual(len(policy[group][key]['sha256']),64)

    def test_mac_profile_still_validates_without_rewriting_devices(self):
        _,host=host_policy(ROOT,'Darwin','arm64')
        value=release.default_profile(ROOT)
        value['phone']['template']=host['phone_template']
        self.assertIs(release.validate(value),value)
        value['phone']['template']='android35-x86_64'
        self.assertIs(release.validate(value),value)

    def test_unimplemented_hosts_are_explicitly_refused(self):
        for os_name,arch in [('Linux','aarch64'),('Windows','AMD64'),('Android','aarch64')]:
            with self.assertRaises(ValueError):host_policy(ROOT,os_name,arch)

if __name__=='__main__':unittest.main()
