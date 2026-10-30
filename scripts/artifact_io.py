"""Streaming content identity shared by runtime packaging and verification."""
import hashlib
from pathlib import Path


def digest(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source,'sha256').hexdigest()
