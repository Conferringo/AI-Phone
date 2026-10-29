"""Invoke SDK Java entry points without the upstream launcher's unquoted paths."""
import os
from pathlib import Path
import subprocess
import sys

ENTRY_POINTS = {'sdkmanager': 'com.android.sdklib.tool.sdkmanager.SdkManagerCli', 'avdmanager': 'com.android.sdklib.tool.AvdManagerCli'}


def command(manager, env, args=()):
    manager = Path(manager)
    name = manager.name
    if name not in ENTRY_POINTS: raise ValueError('Unsupported Android SDK command')
    library = manager.parent.parent / 'lib' / (name + '-classpath.jar')
    java = Path(env['JAVA_HOME']) / 'bin/java'
    if not java.is_file() or not library.is_file(): raise ValueError('Android SDK Java entry point is missing')
    return [str(java), '-Djava.awt.headless=true', '-Dcom.android.sdklib.toolsdir=' + str(manager.parent.parent), '-classpath', str(library), ENTRY_POINTS[name], *args]


if __name__ == '__main__':
    name = sys.argv[1]
    path = Path(os.environ['ANDROID_HOME']) / 'cmdline-tools/latest/bin' / name
    sys.exit(subprocess.run(command(path, os.environ, sys.argv[2:])).returncode)
