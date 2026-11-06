"""Temporary caller adapter; Rust owns the complete runtime environment policy."""
import json
from pathlib import Path
import subprocess


def runtime_env(root):
    from prebuilt_services import runtime_environment_command
    # A restored data directory contains no executable; this installation owns the helper.
    code = Path(__file__).resolve().parent.parent
    result = subprocess.run([*runtime_environment_command(code), '--runtime-environment',
                             str(root), str(Path.home())], cwd=root, capture_output=True, text=True)
    if result.returncode:
        raise ValueError(result.stderr.strip())
    return json.loads(result.stdout)
