"""Recognize exact public self-test strings; never waive a complete vendor file."""
from functools import cache
import hashlib
import json
from pathlib import Path

WINDOW = 4096
PREFIX = b'-----BEGIN '


@cache
def known_vectors():
    catalog=json.loads((Path(__file__).resolve().parent.parent/'config/public-crypto-vectors.json').read_text())
    if catalog['format']!=1:raise ValueError('Unsupported public-vector catalog')
    accepted={}
    for row in catalog['vectors']:
        if not 0<row['size']<=WINDOW:raise ValueError('Public vector exceeds the scanning window')
        accepted.setdefault(row['size'],set()).add(row['sha256'])
    return accepted


def mask_public_vectors(data):
    accepted=known_vectors();cursor=0;masked=None
    while (start:=data.find(PREFIX,cursor))>=0:
        cursor=start+len(PREFIX)
        end=data.find(b'\0',start,start+WINDOW)
        if end<0:continue
        length=end+1-start
        if hashlib.sha256(data[start:end+1]).hexdigest() in accepted.get(length,()):
            if masked is None:masked=bytearray(data)
            masked[start:end+1]=b' '*length
    return data if masked is None else bytes(masked)
