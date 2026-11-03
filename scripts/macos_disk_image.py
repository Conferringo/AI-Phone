"""Package a reviewed application directly into one verified compressed image."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

def _run_hdiutil(operation,*arguments):
    try:
        subprocess.run(['hdiutil',operation,*arguments],check=True,capture_output=True,text=True)
    except subprocess.CalledProcessError as error:
        diagnostic=f"{error.stderr or ''}\n{error.stdout or ''}".strip() or 'no native diagnostic output'
        error.add_note(f'DMG {operation} failed (exit {error.returncode}): {diagnostic}')
        raise

def build(application,output):
    application=Path(application);output=Path(output)
    output.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.dmg-build-',dir=output.parent) as folder:
        folder=Path(folder);content=folder/'contents';content.mkdir()
        # Both paths are on the checkout's filesystem. Link immutable reviewed
        # bytes for staging; do not duplicate a multi-gigabyte runtime tree.
        shutil.copytree(application,content/application.name,copy_function=os.link)
        (content/'Applications').symlink_to('/Applications',target_is_directory=True)
        image=folder/'client.dmg'
        _run_hdiutil('create','-srcfolder',str(content),'-volname','AI Phone','-format','ULMO',str(image))
        _run_hdiutil('verify',str(image))
        image.replace(output)
