"""Offline OCR dependency profile and native-service embedding contract."""
import argparse
import json
from pathlib import Path
from functools import partial
import linux_dependencies as dependencies
from linux_dependencies import MAX_FILES, MAX_BYTES, RESERVE, DependencyProfile, elf_check, checked_identity
from host_platform import host_key

PREFIX = 'runtime/services/ocr/'
LOADER = 'lib/ld-linux-x86-64.so.2'
REQUIRED = frozenset({'bin/tesseract', LOADER, 'lib/libtesseract.so.5', 'tessdata/eng.traineddata',
    'tessdata/chi_sim.traineddata', 'tessdata/configs/tsv', 'python/PIL/__init__.py',
    'python/PIL/_imaging.cpython-313-x86_64-linux-gnu.so', 'source-records.json', 'licenses/tesseract-ocr.copyright'})
SOURCE = ('config/linux-ocr.json', 'scripts/linux_ocr_runtime.py', 'scripts/linux_dependencies.py',
    'scripts/linux_dependency_factory.py',
    'scripts/phone-vision/tesseract.py', 'scripts/linux-ocr/run.py', 'scripts/linux-ocr/tesseract.sh',
    'scripts/linux-ocr/vision.sh', 'scripts/linux-ocr/Dockerfile', 'scripts/linux-ocr/collect.py')
PROFILE = DependencyProfile('config/linux-ocr.json', SOURCE,
    frozenset({'bin', 'lib', 'tessdata', 'python', 'licenses', 'source-records.json'}), REQUIRED,
    (('run.py', 'scripts/linux-ocr/run.py', 0o644),
     ('launchers/tesseract', 'scripts/linux-ocr/tesseract.sh', 0o755),
     ('vision.py', 'scripts/phone-vision/tesseract.py', 0o644)))
archive_member = partial(dependencies.archive_member, profile=PROFILE)
bundled_records = partial(dependencies.bundled_records, prefix=PREFIX)
build = partial(dependencies.build, profile=PROFILE)
prepare = partial(dependencies.prepare, profile=PROFILE, prefix=PREFIX)


def verify(root, folder, *, host=None):
    return dependencies.verify(root, folder, PROFILE, host=host or host_key())


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--destination', type=Path, required=True)
    args = parser.parse_args()
    value = build(args.root, args.archive, args.destination)
    print(json.dumps({'host': value['host'], 'files': len(value['files']), 'bytes': sum(row['size'] for row in value['files'])}))
