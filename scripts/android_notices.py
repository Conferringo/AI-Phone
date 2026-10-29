"""Check Android's resolved runtime coordinates against reviewed third-party notices."""
import json
from pathlib import Path
import subprocess


def compare_graph(expected, actual):
    reviewed={row['package'] for row in expected['packages']}
    resolved={':'.join((row['group'],row['name'],row['version'])) for row in actual}
    if reviewed!=resolved:
        raise ValueError('Android dependency notices are stale; added='+str(sorted(resolved-reviewed))+'; removed='+str(sorted(reviewed-resolved)))


def verify(root, env):
    root=Path(root)
    graph=root/'data/build/android-license-graph.json'
    graph.parent.mkdir(parents=True,exist_ok=True)
    project=root/'apps/client/src-tauri/gen/android'
    command=project/('gradlew.bat' if __import__('os').name=='nt' else 'gradlew')
    subprocess.run([str(command),'--project-dir',str(project),'--init-script',str(root/'apps/client/android/licenses.init.gradle'),
                    ':app:aiPhoneLicenseGraph','--no-configuration-cache','--console=plain'],
                   env={**env,'AI_PHONE_LICENSE_GRAPH':str(graph)},check=True)
    compare_graph(json.loads((root/'config/android-dependency-notices.json').read_text()),json.loads(graph.read_text()))
