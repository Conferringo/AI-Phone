"""One deployment target for the client and its native helper binaries."""
import json
from pathlib import Path
import platform
import re
import subprocess


def minimum_version(root):
    config = json.loads((Path(root) / 'apps/client/src-tauri/tauri.conf.json').read_text())
    return config['bundle']['macOS']['minimumSystemVersion']


def swift_target(root):
    arch = {'arm64': 'arm64', 'x86_64': 'x86_64'}.get(platform.machine())
    if not arch:
        raise ValueError('Unsupported macOS helper architecture')
    return arch + '-apple-macosx' + minimum_version(root)


def version_parts(value):
    return tuple(map(int, (value.split('.') + ['0', '0'])[:3]))


def verify_targets(root, artifacts):
    maximum = version_parts(minimum_version(root))
    result = {}
    for role, path in artifacts.items():
        output = subprocess.check_output(['xcrun', 'vtool', '-show-build', str(path)], text=True)
        versions = re.findall(r'^\s*minos (\d+\.\d+(?:\.\d+)?)\s*$', output, re.M)
        if len(versions) != 1:
            raise ValueError('Native macOS deployment target is missing')
        if version_parts(versions[0]) > maximum:
            raise ValueError('Native helper requires a newer macOS than the client')
        result[role] = versions[0]
    return result


if __name__ == '__main__':
    print(swift_target(Path(__file__).resolve().parent.parent))
