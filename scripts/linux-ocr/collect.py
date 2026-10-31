"""Maintainer factory: preserve the OCR closure, wheel and attribution."""
from pathlib import Path
import shutil
import zipfile
from factory import archive, native_closure, package_records, sha

PILLOW_SHA = '5f56499c1a74d2b862beb0c33b8016f99d095858455df9d21583bbbc1c15b582'


def assemble(root):
    files = native_closure(root, {'tesseract': '/usr/bin/tesseract'})
    (root / 'tessdata/configs').mkdir(parents=True)
    for language in ('eng', 'chi_sim', 'osd'):
        shutil.copyfile('/usr/share/tesseract-ocr/5/tessdata/' + language + '.traineddata', root / 'tessdata' / (language + '.traineddata'))
    shutil.copyfile('/usr/share/tesseract-ocr/5/tessdata/configs/tsv', root / 'tessdata/configs/tsv')
    wheel = next(root.glob('pillow*.whl'))
    if sha(wheel) != PILLOW_SHA: raise ValueError('Pillow wheel differs from the reviewed PyPI bytes')
    with zipfile.ZipFile(wheel) as source: source.extractall(root / 'python')
    package_records(root, files, wheel={'file': wheel.name, 'sha256': PILLOW_SHA}); wheel.unlink()


if __name__ == '__main__':
    root = Path('/out'); assemble(root)
    destination = Path('/export'); destination.mkdir()
    archive(root, destination / 'linux-x64-ocr.tar.gz')
