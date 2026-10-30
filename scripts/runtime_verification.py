"""Required OS environment only; private state is never a verification fixture."""
import os
from pathlib import Path


def isolated_environment(root):
    root = Path(root).resolve()
    if (root / '.env').exists():
        raise ValueError('Use an isolated runtime without existing user state')
    if (root / 'data').exists() and any((root / 'data').iterdir()):
        raise ValueError('Use an isolated runtime without existing user state')
    allowed = ('HOME', 'USERPROFILE', 'SYSTEMROOT', 'TEMP', 'TMP', 'TMPDIR', 'LANG')
    environment = {k: v for k, v in os.environ.items() if k.upper() in allowed}
    environment.update(PATH=os.defpath, AI_PHONE_PROJECT_ROOT=str(root))
    return environment
