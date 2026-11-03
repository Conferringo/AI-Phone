"""The packaging adapter preserves sealed runtime ELF bytes and delegates GUI edits."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


@unittest.skipUnless(os.name=='posix','The Linux packaging adapter requires a POSIX shell')
class AppImageRuntimeIntegrityTests(unittest.TestCase):
    def test_runtime_elf_is_unchanged_while_other_binary_edits_and_failures_are_delegated(self):
        adapter=Path(__file__).with_name('linux-appimage-patchelf.sh')
        with tempfile.TemporaryDirectory(prefix='appimage path ') as temporary:
            root=Path(temporary);tool=root/'real-patchelf'
            tool.write_text('#!/bin/sh\nfor target do :; done\ncase "$1" in\n--print-rpath) echo original-rpath;;\n--set-rpath) printf patched >> "$target";;\n*) exit 23;;\nesac\n');tool.chmod(0o755)
            env={**os.environ,'AI_PHONE_REAL_PATCHELF':str(tool)}
            sealed=root/'client.AppDir/usr/lib/AI Phone/host/tools/bin/python3';sealed.parent.mkdir(parents=True);sealed.write_bytes(b'sealed runtime')
            gui=root/'client.AppDir/usr/bin/ai-phone-client';gui.parent.mkdir(parents=True);gui.write_bytes(b'gui')
            for path in (sealed,gui):
                subprocess.run(['sh',str(adapter),'--set-rpath','$ORIGIN/../lib',str(path)],env=env,check=True)
                self.assertEqual(subprocess.check_output(['sh',str(adapter),'--print-rpath',str(path)],env=env,text=True).strip(),'original-rpath')
            self.assertEqual(sealed.read_bytes(),b'sealed runtime');self.assertEqual(gui.read_bytes(),b'guipatched')
            result=subprocess.run(['sh',str(adapter),'--unknown',str(sealed)],env=env,check=False)
            self.assertEqual(result.returncode,23)


if __name__=='__main__':unittest.main()
