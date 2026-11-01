"""Lossless local-role OCI representation transform; capsule and role formats stay owned elsewhere.

transform(source, destination, records) accepts the same seekable compressed fresh
Docker-export FD for every pass and a raw-TAR writer (the caller owns compression
and publication). records is the four-role mapping already bound by
container_images.inventory, including ref/source_digest/export_digest/config_digest/
size. Neither FD is closed, records is unchanged, and failure may leave partial
raw output which the caller must discard. Returned images are the updated role
records; the caller constructs and validates its existing IMAGES-MANIFEST.
"""
import copy
import gzip
import hashlib
import io
import json
from pathlib import PurePosixPath
import tarfile

from build_capsule_codec import read_tar
from container_images import MAX_ARCHIVE, MAX_METADATA, MAX_RAW, ROLES, checked_image, image_ref

CHUNK = 1024**2
MAX_LAYER = MAX_ARCHIVE
MAX_ENTRIES = 20000
LOCAL = ('workspace', 'desktop')
GZIP = 'application/vnd.oci.image.layer.v1.tar+gzip'
RAW = 'application/vnd.oci.image.layer.v1.tar'
IMAGE = 'application/vnd.oci.image.manifest.v1+json'
INDEX = 'application/vnd.oci.image.index.v1+json'
DOCKER_IMAGE = 'application/vnd.docker.distribution.manifest.v2+json'
DOCKER_INDEX = 'application/vnd.docker.distribution.manifest.list.v2+json'
IMAGES = (IMAGE, DOCKER_IMAGE)
INDEXES = (INDEX, DOCKER_INDEX)
LAYERS = {GZIP: GZIP, RAW: RAW,
          'application/vnd.docker.image.rootfs.diff.tar.gzip': GZIP,
          'application/vnd.docker.image.rootfs.diff.tar': RAW}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return 'sha256:' + hashlib.sha256(data).hexdigest()


def blob(identity):
    require(isinstance(identity, str) and len(identity) == 71 and identity.startswith('sha256:'), 'Invalid OCI digest')
    require(all(c in '0123456789abcdef' for c in identity[7:]), 'Invalid OCI digest')
    return 'blobs/sha256/' + identity[7:]


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':')).encode()


def header(name, size):
    entry = tarfile.TarInfo(name)
    entry.mode = 0o644
    entry.mtime = 0
    entry.size = size
    return entry


class HashReader:
    def __init__(self, source):
        self.source = source
        self.sha = hashlib.sha256()
        self.bytes = 0

    def read(self, size=-1):
        require(type(size) is int and 0 <= size <= CHUNK, 'Unbounded OCI read')
        data = self.source.read(size)
        self.sha.update(data)
        self.bytes += len(data)
        return data

    def identity(self):
        return 'sha256:' + self.sha.hexdigest()


def drain(source):
    while source.read(CHUNK):
        pass


def checked_entry(entry, seen):
    path = PurePosixPath(entry.name)
    require(path.as_posix() == entry.name.rstrip('/') and not path.is_absolute(), 'Invalid OCI archive path')
    require('..' not in path.parts and '\\' not in entry.name and not any(ord(c) < 32 for c in entry.name), 'Invalid OCI archive path')
    require(entry.name not in seen and 0 <= entry.size <= MAX_LAYER, 'Duplicate or oversized OCI member')
    require(entry.isfile() or entry.isdir(), 'OCI member must be a regular file or directory')
    if entry.isdir():
        require(entry.name.rstrip('/') in ('blobs', 'blobs/sha256'), 'Unexpected OCI directory')
    else:
        require(entry.name in ('oci-layout', 'index.json', 'manifest.json') or entry.name.startswith('blobs/sha256/'), 'Unexpected OCI member')
        if entry.name.startswith('blobs/sha256/'):
            require(blob('sha256:' + path.name) == entry.name, 'Invalid OCI blob path')


def metadata(data):
    try:
        return json.loads(data)
    except (UnicodeDecodeError, ValueError):
        return None


def raw_identity(source):
    result = HashReader(source)
    while result.read(CHUNK):
        require(result.bytes <= MAX_LAYER, 'OCI layer exceeds raw byte budget')
    return {'digest': result.identity(), 'size': result.bytes}


def checked_raw(source, media_type, expected):
    if media_type == GZIP:
        with gzip.GzipFile(fileobj=source, mode='rb') as plain:
            result = raw_identity(plain)
    else:
        result = raw_identity(source)
    require(result['digest'] == expected, 'Config diff_id mismatch')
    return result


def scan_body(source, binding):
    packed = HashReader(source)
    if binding is not None:
        return packed, None, checked_raw(packed, *binding)
    chunks = []
    while data := packed.read(CHUNK):
        if packed.bytes <= MAX_METADATA:
            chunks.append(data)
        else:
            chunks.clear()
    data = b''.join(chunks) if packed.bytes <= MAX_METADATA else None
    return packed, data, None


def scan(source, bindings):
    """Bounded actual-byte catalog; only small metadata is retained in memory."""
    files, small, verified, seen = {}, {}, {}, set()
    total = metadata_bytes = 0
    with read_tar(source, MAX_RAW, stream=True) as archive:
        for position, entry in enumerate(archive):
            require(position < MAX_ENTRIES, 'OCI archive exceeds entry budget')
            checked_entry(entry, seen)
            seen.add(entry.name)
            if entry.isdir():
                continue
            total += entry.size
            require(total <= MAX_ARCHIVE, 'OCI archive exceeds byte budget')
            with archive.extractfile(entry) as body:
                packed, data, raw = scan_body(body, bindings.get(entry.name))
            require(packed.bytes == entry.size, 'Incomplete OCI source member')
            if entry.name.startswith('blobs/sha256/'):
                require(blob(packed.identity()) == entry.name, 'Source blob digest mismatch')
            files[entry.name] = {'size': entry.size, 'digest': packed.identity()}
            if data is not None and metadata(data) is not None:
                metadata_bytes += len(data)
                require(metadata_bytes <= MAX_METADATA, 'OCI metadata exceeds byte budget')
                small[entry.name] = data
            if raw is not None:
                verified[entry.name] = raw
    require(set(verified) == set(bindings), 'Missing OCI layer')
    return {'files': files, 'small': small, 'verified': verified}


def descriptor(value, catalog):
    require(isinstance(value, dict), 'Invalid OCI descriptor')
    name = blob(value.get('digest'))
    require(type(value.get('size')) is int and value['size'] >= 0, 'Invalid OCI descriptor size')
    require(name in catalog['files'] and catalog['files'][name]['size'] == value['size'], 'OCI descriptor size or closure mismatch')
    return name


def graph(value, kind):
    kinds = IMAGES if kind in IMAGES else INDEXES if kind in INDEXES else (kind,)
    require(isinstance(value, dict) and value.get('schemaVersion') == 2 and value.get('mediaType') in kinds, 'Invalid OCI graph')
    return value


def graph_children(value, kind):
    graph(value, kind)
    if kind in IMAGES:
        require(isinstance(value.get('layers'), list), 'Invalid OCI layers')
        children = [value.get('config'), *value['layers']]
    else:
        require(isinstance(value.get('manifests'), list), 'Invalid OCI nested index')
        children = list(value['manifests'])
    if 'subject' in value:
        children.append(value['subject'])
    return children


def add_binding(bindings, layer, diff_id, catalog):
    name = descriptor(layer, catalog)
    blob(diff_id)
    require(layer.get('mediaType') in LAYERS, 'Unsupported OCI filesystem layer')
    binding = (LAYERS[layer['mediaType']], diff_id)
    require(name not in bindings or bindings[name] == binding, 'Inconsistent config diff_id binding')
    bindings[name] = binding


def checked_config(value, layers):
    require(isinstance(value, dict) and value.get('os') == 'linux' and value.get('architecture') in ('arm64', 'amd64'), 'Unsupported OCI image config')
    rootfs = value.get('rootfs')
    require(isinstance(rootfs, dict) and rootfs.get('type') == 'layers', 'Invalid OCI rootfs')
    require(isinstance(rootfs.get('diff_ids'), list) and len(rootfs['diff_ids']) == len(layers), 'Config diff_id count mismatch')
    return rootfs['diff_ids']


def role_graphs(records, objects, catalog):
    require(isinstance(records, dict) and set(records) == set(ROLES), 'Provide the four image roles')
    graphs, bindings, architectures = {}, {}, set()
    for role in ROLES:
        record = records[role]
        checked_image(role, record)
        value = graph(objects.get(blob(record['export_digest'])), IMAGE)
        config_name = descriptor(value.get('config'), catalog)
        require(config_name == blob(record['config_digest']), 'OCI manifest/config binding mismatch')
        config = objects.get(config_name)
        layers = value.get('layers')
        require(isinstance(layers, list), 'Invalid OCI layers')
        ids = checked_config(config, layers)
        architectures.add(config['architecture'])
        for layer, diff_id in zip(layers, ids):
            add_binding(bindings, layer, diff_id, catalog)
        graphs[role] = value
    require(len(architectures) == 1, 'Source image architecture mismatch')
    return graphs, bindings, architectures.pop()


def role_reference(value):
    annotations = value.get('annotations', {})
    require(isinstance(annotations, dict), 'Invalid OCI annotations')
    reference = annotations.get('io.containerd.image.name', '')
    require(isinstance(reference, str), 'Invalid OCI role reference')
    return reference.removeprefix('docker.io/')


def index_binding(value, records, seen):
    annotations = value.get('annotations', {})
    reference = role_reference(value)
    if not reference:
        return
    role = next((role for role in ROLES if records[role]['ref'] == reference), None)
    require(role is not None and role not in seen, 'Unexpected or duplicate OCI role reference')
    record = records[role]
    require(value['digest'] == record['export_digest'], 'OCI role digest mismatch')
    require(annotations.get('org.opencontainers.image.ref.name', reference.split(':', 1)[1]) == reference.split(':', 1)[1], 'OCI role annotation mismatch')
    seen.add(role)


def checked_index(objects, records, catalog):
    value = graph(objects.get('index.json'), INDEX)
    seen = set()
    for item in graph_children(value, INDEX):
        descriptor(item, catalog)
        index_binding(item, records, seen)
    require(seen == set(ROLES), 'Incomplete OCI role index')
    return value


def checked_docker(objects, records, graphs):
    rows = objects.get('manifest.json')
    require(isinstance(rows, list) and len(rows) == len(ROLES), 'Invalid Docker image inventory')
    seen = set()
    for row in rows:
        require(isinstance(row, dict) and isinstance(row.get('RepoTags'), list) and len(row['RepoTags']) == 1, 'Invalid Docker role row')
        role = next((role for role in ROLES if records[role]['ref'] == row['RepoTags'][0]), None)
        require(role is not None and role not in seen, 'Unexpected or duplicate Docker role')
        require(row.get('Config') == blob(records[role]['config_digest']), 'Docker config binding mismatch')
        require(row.get('Layers') == [blob(layer['digest']) for layer in graphs[role]['layers']], 'Docker layer binding mismatch')
        seen.add(role)
    return rows


def reserved_objects(index, records, objects, catalog):
    """Keep every nonlocal image/proof descendant and explicit subject target."""
    local = {records[role]['ref'] for role in LOCAL}
    pending = [item for item in index['manifests'] if role_reference(item) not in local]
    if 'subject' in index:
        pending.append(index['subject'])
    reserved = set()
    while pending:
        item = pending.pop()
        name = descriptor(item, catalog)
        if name in reserved:
            continue
        reserved.add(name)
        kind = item.get('mediaType')
        if kind in (*IMAGES, *INDEXES):
            pending.extend(graph_children(objects.get(name), kind))
        annotations = item.get('annotations', {})
        require(isinstance(annotations, dict), 'Invalid OCI annotations')
        subject = annotations.get('io.containerd.manifest.subject')
        if subject is not None:
            subject_name = blob(subject)
            require(subject_name in catalog['files'], 'Missing OCI proof subject')
            pending.append({'digest': subject, 'size': catalog['files'][subject_name]['size'], 'mediaType': IMAGE})
    return reserved


def replacements_for_role(role, records, graph_value, converted, retained, catalog):
    record = copy.deepcopy(records[role])
    value = copy.deepcopy(graph_value)
    changed = False
    retained.add(blob(record['config_digest']))
    for layer in value['layers']:
        name = blob(layer['digest'])
        if name in converted:
            layer.update(converted[name], mediaType=RAW)
            changed = True
        else:
            retained.add(name)
    if not changed:
        retained.add(blob(record['export_digest']))
        return record, None
    value['mediaType'] = IMAGE
    value['config']['mediaType'] = 'application/vnd.oci.image.config.v1+json'
    for layer in value['layers']:
        layer['mediaType'] = LAYERS[layer['mediaType']]
    data = encoded(value)
    identity = digest(data)
    record.update(export_digest=identity, ref=image_ref(role, identity), size=sum(layer['size'] for layer in value['layers']) + catalog['files'][blob(record['config_digest'])]['size'])
    return record, data


def updated_index(index, records, updated, replacements):
    value = copy.deepcopy(index)
    by_ref = {records[role]['ref']: role for role in LOCAL}
    for item in value['manifests']:
        annotations = item.get('annotations', {})
        reference = role_reference(item)
        if reference in by_ref:
            record = updated[by_ref[reference]]
            name = blob(record['export_digest'])
            if name in replacements:
                item.update(digest=record['export_digest'], size=len(replacements[name]), mediaType=IMAGE)
            annotations.update({'io.containerd.image.name': 'docker.io/' + record['ref'], 'org.opencontainers.image.ref.name': record['ref'].split(':', 1)[1]})
    return encoded(value)


def updated_docker(rows, records, updated, converted):
    value = copy.deepcopy(rows)
    by_ref = {records[role]['ref']: role for role in LOCAL}
    for row in value:
        if row['RepoTags'][0] in by_ref:
            role = by_ref[row['RepoTags'][0]]
            row['RepoTags'] = [updated[role]['ref']]
            row['Layers'] = [blob(converted[name]['digest']) if name in converted else name for name in row['Layers']]
    return encoded(value)


def conversion_plan(catalog, records, objects, graphs, index, rows, reserved, bindings):
    local_layers = {blob(layer['digest']) for role in LOCAL for layer in graphs[role]['layers']}
    converted = {name: catalog['verified'][name] for name in local_layers - reserved if bindings[name][0] == GZIP}
    retained = {'oci-layout', *reserved}
    updated = copy.deepcopy(records)
    replacements = {}
    for role in LOCAL:
        updated[role], data = replacements_for_role(role, records, graphs[role], converted, retained, catalog)
        if data is not None:
            replacements[blob(digest(data))] = data
    replacements['index.json'] = updated_index(index, records, updated, replacements)
    replacements['manifest.json'] = updated_docker(rows, records, updated, converted)
    return {'images': updated, 'retained': retained, 'replacements': replacements, 'converted': converted, 'source_files': catalog['files']}


def emit_layer(target, body, name, update, emitted):
    with gzip.GzipFile(fileobj=body, mode='rb') as decoded:
        plain = HashReader(decoded)
        if name not in emitted:
            target.addfile(header(name, update['size']), plain)
        while plain.bytes < update['size']:
            require(plain.read(min(CHUNK, update['size'] - plain.bytes)), 'Incomplete converted OCI layer')
        require(plain.read(1) == b'', 'Converted OCI layer exceeds verified size')
        require(plain.bytes == update['size'] and plain.identity() == update['digest'], 'Converted OCI layer changed')


def emit_source(target, body, entry, plan, emitted):
    packed = HashReader(body)
    name = entry.name
    if name in plan['converted']:
        update = plan['converted'][name]
        output_name = blob(update['digest'])
        emit_layer(target, packed, output_name, update, emitted)
        emitted.add(output_name)
    elif name in plan['retained'] and name not in emitted:
        target.addfile(header(name, entry.size), packed)
        emitted.add(name)
    drain(packed)
    require({'size': packed.bytes, 'digest': packed.identity()} == plan['source_files'][name], 'OCI source changed between passes')


def emit_archive(source, destination, plan):
    emitted, seen = set(), set()
    with read_tar(source, MAX_RAW, stream=True) as archive, tarfile.open(fileobj=destination, mode='w|', bufsize=CHUNK) as target:
        for position, entry in enumerate(archive):
            require(position < MAX_ENTRIES, 'OCI archive exceeds entry budget')
            checked_entry(entry, seen)
            seen.add(entry.name)
            if entry.isdir():
                continue
            require(entry.name in plan['source_files'], 'OCI source changed between passes')
            require(entry.size == plan['source_files'][entry.name]['size'], 'OCI source size changed between passes')
            with archive.extractfile(entry) as body:
                emit_source(target, body, entry, plan, emitted)
        require({name for name in seen if name in plan['source_files']} == set(plan['source_files']), 'Incomplete OCI emission source')
        for name, data in sorted(plan['replacements'].items()):
            if name not in emitted:
                target.addfile(header(name, len(data)), io.BytesIO(data))
                emitted.add(name)
    expected = plan['retained'] | set(plan['replacements']) | {blob(value['digest']) for value in plan['converted'].values()}
    require(emitted == expected, 'OCI emission closure mismatch')
    return len(emitted)


def transform(source, destination, records):
    require(source is not destination and source.seekable(), 'A separate seekable compressed OCI source is required')
    initial = scan(source, {})
    objects = {name: metadata(data) for name, data in initial['small'].items()}
    require(objects.get('oci-layout') == {'imageLayoutVersion': '1.0.0'}, 'Invalid OCI layout')
    graphs, bindings, architecture = role_graphs(records, objects, initial)
    index = checked_index(objects, records, initial)
    rows = checked_docker(objects, records, graphs)
    reserved = reserved_objects(index, records, objects, initial)
    catalog = scan(source, bindings)
    require(catalog['files'] == initial['files'], 'OCI source changed between passes')
    plan = conversion_plan(catalog, records, objects, graphs, index, rows, reserved, bindings)
    members = emit_archive(source, destination, plan)
    return {'images': plan['images'], 'architecture': architecture, 'members': members,
            'verified_layers': catalog['verified'],
            'preserved_object_hashes': {name: catalog['files'][name]['digest'] for name in plan['retained']}}
