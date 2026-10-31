"""Package a reviewed application directly into one verified compressed image."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

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
        subprocess.run(['hdiutil','create','-quiet','-srcfolder',str(content),'-volname','AI Phone','-format','UDZO',str(image)],check=True)
        subprocess.run(['hdiutil','verify',str(image)],check=True,stdout=subprocess.DEVNULL)
        image.replace(output)
