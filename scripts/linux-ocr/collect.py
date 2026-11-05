"""Maintainer factory: native OCR closure and attribution."""
from pathlib import Path
import shutil
from factory import archive, native_closure, package_records


def assemble(root):
    files = native_closure(root, {'tesseract': '/usr/bin/tesseract'})
    (root / 'tessdata/configs').mkdir(parents=True)
    for language in ('eng', 'chi_sim'):
        shutil.copyfile('/usr/share/tesseract-ocr/5/tessdata/' + language + '.traineddata', root / 'tessdata' / (language + '.traineddata'))
    shutil.copyfile('/usr/share/tesseract-ocr/5/tessdata/configs/tsv', root / 'tessdata/configs/tsv')
    package_records(root, files)


if __name__ == '__main__':
    root = Path('/out'); assemble(root)
    destination = Path('/export'); destination.mkdir()
    archive(root, destination / 'linux-x64-ocr.tar.gz')
