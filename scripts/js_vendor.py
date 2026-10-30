"""Exact public registry bytes may retain upstream build paths, never credentials."""
import hashlib
import json
from pathlib import Path
import re
from client_host_entry import canonical_name
from release_package import SECRET


def read_approvals(root):
    path = Path(root) / 'config/js-runtime-vendor.json'
    if not path.exists() and not path.is_symlink():
        return {}
    if path.is_symlink() or path.stat().st_size > 65536:
        raise ValueError('Invalid JS vendor provenance')
    value = json.loads(path.read_text())
    result = {}
    for row in asset_records(value):
        key, identity = approved_record(row)
        if key in result:
            raise ValueError('Duplicate JS vendor asset')
        result[key] = identity
    return result


def asset_records(value):
    if not isinstance(value, dict) or type(value.get('format')) is not int or value['format'] != 1:
        raise ValueError('Invalid JS vendor provenance')
    rows = value.get('assets')
    if not isinstance(rows, list) or len(rows) > 32:
        raise ValueError('Invalid JS vendor asset list')
    return rows


def approved_record(row):
    if not isinstance(row, dict):
        raise ValueError('Invalid JS vendor asset')
    for field in ('package', 'version', 'file', 'sha256', 'archive_url', 'archive_integrity', 'archive_sha256'):
        if not isinstance(row.get(field), str) or not row[field]:
            raise ValueError('Incomplete JS vendor provenance')
    canonical_name(row['file'])
    if not row['archive_url'].startswith('https://registry.npmjs.org/'):
        raise ValueError('JS vendor origin must be the pinned public registry')
    if any(re.fullmatch('[0-9a-f]{64}', row[key]) is None for key in ('sha256', 'archive_sha256')):
        raise ValueError('Invalid JS vendor digest')
    if re.fullmatch(r'sha512-[A-Za-z0-9+/]{86}==', row['archive_integrity']) is None:
        raise ValueError('Invalid JS vendor archive integrity')
    return (row['package'], row['version'], row['file']), row['sha256']


def dependency_bytes(root, package, meta, source, name, approvals):
    data = source.read_bytes()
    if SECRET.search(data) or SECRET.search(data.replace(b'\0', b'')):
        raise ValueError('Possible credential in JS dependency')
    key = (meta['name'], meta['version'], source.relative_to(package).as_posix())
    if hashlib.sha256(data).hexdigest() == approvals.get(key):
        return data
    from prebuilt_web import public_runtime_file
    return public_runtime_file(root, source, source.relative_to(root), name)
