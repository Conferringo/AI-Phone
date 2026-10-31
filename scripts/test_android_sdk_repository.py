"""Stable SDK input selection never infers a channel from package order/build ID."""
import unittest
import android_sdk_repository as repository


def package(channel,version,file,license='android-sdk-license'):
    return f'''<remotePackage path="emulator"><revision><major>37</major><minor>{version}</minor><micro>1</micro></revision>
    <channelRef ref="{channel}"/><uses-license ref="{license}"/><archives><archive><host-os>linux</host-os><host-arch>x64</host-arch>
    <complete><size>10</size><checksum type="sha1">{'a'*40}</checksum><url>{file}</url></complete></archive></archives></remotePackage>'''


class SdkRepositoryTests(unittest.TestCase):
    def test_stable_selection_uses_channel_license_and_host_not_order_or_build_number(self):
        preview=package('channel-2',3,'emulator-linux_x64-100.zip','android-sdk-preview-license')
        stable=package('channel-0',2,'emulator-linux_x64-999.zip')
        for rows in [(preview,stable),(stable,preview)]:
            result=repository.select(('<repository>'+''.join(rows)+'</repository>').encode(),'emulator','linux','x64')
            self.assertEqual(result['url'],'https://dl.google.com/android/repository/emulator-linux_x64-999.zip')
            self.assertEqual(result['version'],'37.2.1');self.assertEqual(result['license_id'],'android-sdk-license')

    def test_preview_only_missing_duplicate_or_wrong_license_cannot_be_promoted(self):
        stable=package('channel-0',2,'emulator-linux_x64-999.zip')
        cases=[package('channel-2',3,'emulator-linux_x64-100.zip'),stable+stable,stable.replace('linux','windows'),
               stable.replace('android-sdk-license','android-sdk-preview-license'),stable.replace('type="sha1"','type="sha256"'),
               stable.replace('emulator-linux_x64-999.zip','../escape.zip')]
        for value in cases:
            with self.subTest(value=value),self.assertRaises(ValueError):repository.select(('<repository>'+value+'</repository>').encode(),'emulator','linux','x64')


if __name__=='__main__':unittest.main()
