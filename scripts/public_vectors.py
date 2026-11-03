"""Recognize exact public cryptographic strings; never waive a vendor file."""
from functools import cache
import hashlib
import json
from pathlib import Path

WINDOW = 4096
PREFIXES = (b'-----BEGIN ', b'sk-')


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
    accepted=known_vectors();masked=None
    for prefix in PREFIXES:
        cursor=0
        while (start:=data.find(prefix,cursor))>=0:
            cursor=start+len(prefix)
            for length,identities in accepted.items():
                end=start+length
                if end<=len(data) and data[end-1]==0 and hashlib.sha256(data[start:end]).hexdigest() in identities:
                    if masked is None:masked=bytearray(data)
                    masked[start:end]=b' '*length
                    break
    return data if masked is None else bytes(masked)
