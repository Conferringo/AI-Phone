"""Build/CI-only preparation of pinned OCR data; never used by a running service."""
import argparse
import json
from pathlib import Path
from client_host import download

ROOT = Path(__file__).resolve().parent.parent


def prepare(destination, root=ROOT):
    configuration=json.loads((Path(root)/'config/ocr-models.json').read_text())
    destination=Path(destination)
    for model in configuration['models']:
        download(model,destination/model['file'])
    return {'models':len(configuration['models']),'source_commit':configuration['source_commit']}


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--destination',type=Path,required=True)
    arguments=parser.parse_args()
    print(json.dumps(prepare(arguments.destination)))
