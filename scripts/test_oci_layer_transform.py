"""Small OCI byte-contract fixtures; no Docker, network, extraction or subprocesses."""
import contextlib
import copy
import gzip
import hashlib
import http.client
import importlib.util
import io
import json
from pathlib import Path
import socket
import ssl
import subprocess
import sys
import tarfile
import unittest
from unittest.mock import patch
import urllib.request


def forbidden(*args, **kwargs):
    raise AssertionError('fixture cannot use network or subprocesses')


DENIED = ('socket.socket', 'socket.create_connection', 'socket.getaddrinfo',
          'subprocess.Popen', 'subprocess.run', 'subprocess.call',
          'subprocess.check_call', 'subprocess.check_output', 'os.system', 'os.popen')
import oci_layer_transform as m
from build_capsule_codec import write_xz

GZIP = 'application/vnd.oci.image.layer.v1.tar+gzip'
RAW = 'application/vnd.oci.image.layer.v1.tar'
IMAGE = 'application/vnd.oci.image.manifest.v1+json'
INDEX = 'application/vnd.oci.image.index.v1+json'
CONFIG = 'application/vnd.oci.image.config.v1+json'
ROLES = ('workspace', 'desktop', 'postgres', 'redis')


def digest(data):
    return 'sha256:' + hashlib.sha256(data).hexdigest()


def blob(identity):
    return 'blobs/sha256/' + identity.removeprefix('sha256:')


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':')).encode()


def header(name, size):
    entry = tarfile.TarInfo(name)
    entry.size = size
    entry.mode = 0o644
    entry.mtime = 0
    return entry


def tar_bytes(files):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode='w') as archive:
        for name, data in files.items():
            archive.addfile(header(name, len(data)), io.BytesIO(data))
    return output.getvalue()


def filesystem_layer(label):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode='w', format=tarfile.PAX_FORMAT) as archive:
        data = b'fixture contents\n'
        entry = header('work/' + label, len(data))
        entry.uid, entry.gid, entry.mode, entry.mtime = 1001, 1002, 0o751, 123456
        entry.pax_headers = {'SCHILY.xattr.user.fixture': 'retained'}
        archive.addfile(entry, io.BytesIO(data))
        symbolic = header('work/symbolic', 0)
        symbolic.type, symbolic.linkname = tarfile.SYMTYPE, label
        archive.addfile(symbolic)
        hard = header('work/hard', 0)
        hard.type, hard.linkname = tarfile.LNKTYPE, 'work/' + label
        archive.addfile(hard)
        archive.addfile(header('work/.wh.removed', 0), io.BytesIO(b''))
        archive.addfile(header('work/.wh..wh..opq', 0), io.BytesIO(b''))
    return output.getvalue()


def add_blob(value, data, media_type):
    identity = digest(data)
    value['files'][blob(identity)] = data
    return {'digest': identity, 'size': len(data), 'mediaType': media_type}


def sync_metadata(value):
    value['files']['index.json'] = encoded(value['index'])
    value['files']['manifest.json'] = encoded(value['docker'])


def fixture(*, duplicate_raw=False, shared_upstream=False, upstream_raw=False):
    value = {'files': {'oci-layout': encoded({'imageLayoutVersion': '1.0.0'})},
             'records': {}, 'graphs': {}, 'plain': {},
             'index': {'schemaVersion': 2, 'mediaType': INDEX, 'manifests': []}, 'docker': []}
    shared = filesystem_layer('shared')
    for role in ROLES:
        raws = [shared] if role == 'workspace' else [shared, shared if duplicate_raw else filesystem_layer('desktop')] if role == 'desktop' else [filesystem_layer(role)]
        if role == 'redis' and (shared_upstream or upstream_raw):
            raws = [shared]
        layers = []
        for ordinal, raw in enumerate(raws):
            layer_type = RAW if role == 'redis' and upstream_raw else GZIP
            data = raw if layer_type == RAW else gzip.compress(raw, mtime=1 if ordinal == 0 else 2)
            descriptor = add_blob(value, data, layer_type)
            layers.append(descriptor)
            value['plain'][blob(descriptor['digest'])] = raw
        config = {'os': 'linux', 'architecture': 'arm64', 'config': {'Env': ['FIXTURE=' + role]},
                  'rootfs': {'type': 'layers', 'diff_ids': [digest(raw) for raw in raws]}}
        config_data = encoded(config)
        config_desc = add_blob(value, config_data, CONFIG)
        graph = {'schemaVersion': 2, 'mediaType': IMAGE, 'config': config_desc, 'layers': layers}
        graph_desc = add_blob(value, encoded(graph), IMAGE)
        reference = 'ai-phone-bundle/' + role + ':sha-' + graph_desc['digest'][7:]
        value['records'][role] = {'ref': reference, 'source_digest': config_desc['digest'],
                                  'export_digest': graph_desc['digest'], 'config_digest': config_desc['digest'],
                                  'size': len(config_data) + sum(row['size'] for row in layers)}
        value['index']['manifests'].append({**graph_desc, 'annotations': {
            'io.containerd.image.name': 'docker.io/' + reference,
            'org.opencontainers.image.ref.name': reference.split(':', 1)[1]}})
        value['docker'].append({'Config': blob(config_desc['digest']), 'RepoTags': [reference],
                                'Layers': [blob(row['digest']) for row in layers]})
        value['graphs'][role] = graph
        if role in ('postgres', 'redis'):
            add_proof(value, role)
    sync_metadata(value)
    return value


def add_proof(value, role, *, nested=False):
    record = value['records'][role]
    original = value['files'][blob(record['export_digest'])]
    subject = {'digest': record['export_digest'], 'size': len(original), 'mediaType': IMAGE}
    statement = {'_type': 'https://in-toto.io/Statement/v0.1',
                 'subject': [{'name': role, 'digest': {'sha256': record['export_digest'][7:]}}],
                 'predicateType': 'https://slsa.dev/provenance/v0.2', 'predicate': {'original': True}}
    layer = add_blob(value, encoded(statement), 'application/vnd.in-toto+json')
    config = add_blob(value, encoded({'architecture': 'unknown', 'os': 'unknown'}), CONFIG)
    graph = {'schemaVersion': 2, 'mediaType': IMAGE, 'config': config, 'layers': [layer], 'subject': subject}
    descriptor = add_blob(value, encoded(graph), IMAGE)
    descriptor['annotations'] = {'io.containerd.manifest.subject': record['export_digest']}
    if nested:
        tree = {'schemaVersion': 2, 'mediaType': INDEX, 'manifests': [descriptor]}
        descriptor = add_blob(value, encoded(tree), INDEX)
    value['index']['manifests'].append(descriptor)
    sync_metadata(value)
    return descriptor


def replace_role_config(value, role, change):
    record = value['records'][role]
    old_digest, old_ref = record['export_digest'], record['ref']
    graph = copy.deepcopy(value['graphs'][role])
    old_config = graph['config']['digest']
    config = json.loads(value['files'][blob(old_config)])
    change(config)
    config_data = encoded(config)
    graph['config'] = add_blob(value, config_data, CONFIG)
    descriptor = add_blob(value, encoded(graph), IMAGE)
    reference = 'ai-phone-bundle/' + role + ':sha-' + descriptor['digest'][7:]
    record.update(ref=reference, export_digest=descriptor['digest'], config_digest=graph['config']['digest'],
                  size=len(config_data) + sum(row['size'] for row in graph['layers']))
    value['graphs'][role] = graph
    for entry in value['index']['manifests']:
        if entry.get('annotations', {}).get('io.containerd.image.name') == 'docker.io/' + old_ref:
            entry.update(descriptor)
            entry['annotations'] = {'io.containerd.image.name': 'docker.io/' + reference,
                                    'org.opencontainers.image.ref.name': reference.split(':', 1)[1]}
    for row in value['docker']:
        if row['RepoTags'] == [old_ref]:
            row.update(Config=blob(graph['config']['digest']), RepoTags=[reference],
                       Layers=[blob(layer['digest']) for layer in graph['layers']])
    value['files'].pop(blob(old_digest))
    value['files'].pop(blob(old_config))
    sync_metadata(value)


def compress(value, codec='gzip', files=None):
    data = tar_bytes(value['files'] if files is None else files)
    if codec == 'gzip':
        return gzip.compress(data, mtime=0)
    target = io.BytesIO()
    with write_xz(target) as output:
        output.write(data)
    return target.getvalue()


def output_files(raw):
    names, files = [], {}
    with tarfile.open(fileobj=io.BytesIO(raw), mode='r:') as archive:
        for entry in archive:
            if entry.isfile():
                names.append(entry.name)
                files[entry.name] = archive.extractfile(entry).read()
    return names, files


def preserved_closure(value):
    found = set()

    def visit(descriptor):
        name = blob(descriptor['digest'])
        if name in found:
            return
        found.add(name)
        data = value['files'][name]
        if descriptor['mediaType'] not in (IMAGE, INDEX):
            return
        obj = json.loads(data)
        if descriptor['mediaType'] == IMAGE:
            visit(obj['config'])
            for child in obj['layers']:
                visit(child)
            if 'subject' in obj:
                visit(obj['subject'])
        else:
            for child in obj['manifests']:
                visit(child)
    local = {value['records'][role]['ref'] for role in ('workspace', 'desktop')}
    for descriptor in value['index']['manifests']:
        reference = descriptor.get('annotations', {}).get('io.containerd.image.name', '').removeprefix('docker.io/')
        if reference not in local:
            visit(descriptor)
    return found


class TransformTests(unittest.TestCase):
    def setUp(self):
        self.block = contextlib.ExitStack()
        self.addCleanup(self.block.close)
        for name in DENIED:
            self.block.enter_context(patch(name, side_effect=forbidden))

    def convert(self, value, codec='gzip', files=None):
        source = io.BytesIO(compress(value, codec, files))
        output = io.BytesIO()
        before = copy.deepcopy(value['records'])
        result = m.transform(source, output, value['records'])
        self.assertEqual(value['records'], before, 'caller records must not be mutated')
        names, emitted = output_files(output.getvalue())
        self.assertEqual(len(names), len(set(names)), 'duplicate emitted archive member')
        self.assertEqual(result['members'], len(names))
        self.assertEqual(set(result['images']), set(ROLES))
        self.assertEqual(result['architecture'], 'arm64')
        expected_layers = {blob(layer['digest']) for graph in value['graphs'].values() for layer in graph['layers']}
        self.assertEqual(set(result['verified_layers']), expected_layers)
        for name in expected_layers:
            self.assertEqual(result['verified_layers'][name], {'digest': digest(value['plain'][name]), 'size': len(value['plain'][name])})
        for name, data in emitted.items():
            if name.startswith('blobs/'):
                self.assertEqual(name, blob(digest(data)))
        for name in preserved_closure(value):
            self.assertEqual(emitted[name], value['files'][name])
            self.assertEqual(result['preserved_object_hashes'][name], digest(value['files'][name]))
        return result, emitted

    def test_closed_two_local_roles_and_unchanged_upstream_proof_bytes(self):
        value = fixture()
        result, emitted = self.convert(value)
        index, rows = json.loads(emitted['index.json']), json.loads(emitted['manifest.json'])
        for role, record in result['images'].items():
            old = value['records'][role]
            self.assertEqual(record['config_digest'], old['config_digest'])
            self.assertEqual(record['source_digest'], old['source_digest'])
            config_name = blob(record['config_digest'])
            self.assertEqual(emitted[config_name], value['files'][config_name])
            graph = json.loads(emitted[blob(record['export_digest'])])
            config = json.loads(emitted[config_name])
            row = next(row for row in rows if row['RepoTags'] == [record['ref']])
            self.assertEqual(row['Config'], config_name)
            self.assertEqual(row['Layers'], [blob(layer['digest']) for layer in graph['layers']])
            descriptor = next(row for row in index['manifests'] if row.get('annotations', {}).get('io.containerd.image.name') == 'docker.io/' + record['ref'])
            self.assertEqual(descriptor['digest'], record['export_digest'])
            self.assertEqual(descriptor['size'], len(emitted[blob(record['export_digest'])]))
            self.assertEqual(descriptor['annotations']['org.opencontainers.image.ref.name'], record['ref'].split(':', 1)[1])
            for position, layer in enumerate(graph['layers']):
                self.assertEqual(layer['size'], len(emitted[blob(layer['digest'])]))
                if role in ('workspace', 'desktop'):
                    self.assertEqual(layer['mediaType'], RAW)
                    self.assertEqual(layer['digest'], config['rootfs']['diff_ids'][position])
                    original_name = blob(value['graphs'][role]['layers'][position]['digest'])
                    self.assertEqual(emitted[blob(layer['digest'])], value['plain'][original_name])
            if role not in ('workspace', 'desktop'):
                self.assertEqual(record, old)

    def test_layer_metadata_whiteouts_and_links_are_identical_bytes(self):
        value = fixture()
        result, emitted = self.convert(value)
        graph = json.loads(emitted[blob(result['images']['desktop']['export_digest'])])
        raw = emitted[blob(graph['layers'][1]['digest'])]
        self.assertEqual(raw, filesystem_layer('desktop'))
        with tarfile.open(fileobj=io.BytesIO(raw), mode='r:') as archive:
            entry = archive.getmember('work/desktop')
            self.assertEqual((entry.uid, entry.gid, entry.mode, entry.mtime), (1001, 1002, 0o751, 123456))
            self.assertEqual(entry.pax_headers['SCHILY.xattr.user.fixture'], 'retained')
            self.assertTrue(archive.getmember('work/symbolic').issym())
            self.assertTrue(archive.getmember('work/hard').islnk())
            self.assertEqual(archive.getmember('work/hard').linkname, 'work/desktop')
            self.assertEqual(archive.getmember('work/.wh.removed').size, 0)
            self.assertEqual(archive.getmember('work/.wh..wh..opq').size, 0)

    def test_different_gzip_representations_with_one_diff_id_emit_once(self):
        value = fixture(duplicate_raw=True)
        result, emitted = self.convert(value)
        local_names = {blob(layer['digest']) for role in ('workspace', 'desktop') for layer in value['graphs'][role]['layers']}
        self.assertEqual(len(local_names), 2)
        identities = {result['verified_layers'][name]['digest'] for name in local_names}
        self.assertEqual(len(identities), 1)
        self.assertIn(blob(next(iter(identities))), emitted)

    def test_local_upstream_shared_gzip_is_retained_for_both_roles(self):
        value = fixture(shared_upstream=True)
        shared = value['graphs']['workspace']['layers'][0]
        result, emitted = self.convert(value)
        name = blob(shared['digest'])
        self.assertEqual(emitted[name], value['files'][name])
        for role in ('workspace', 'desktop', 'redis'):
            graph = json.loads(emitted[blob(result['images'][role]['export_digest'])])
            self.assertEqual(graph['layers'][0], shared)
        self.assertIn(name, result['verified_layers'])
        self.assertEqual(result['verified_layers'][name]['digest'], digest(value['plain'][name]))
        self.assertIn(name, result['preserved_object_hashes'])

    def test_proof_subject_retains_original_local_manifest_graph(self):
        value = fixture()
        descriptor = add_proof(value, 'workspace')
        proof_bytes = value['files'][blob(descriptor['digest'])]
        old_subject = value['records']['workspace']['export_digest']
        result, emitted = self.convert(value)
        self.assertEqual(emitted[blob(descriptor['digest'])], proof_bytes)
        self.assertIn(blob(old_subject), emitted)
        self.assertEqual(json.loads(proof_bytes)['subject']['digest'], old_subject)
        graph = json.loads(emitted[blob(result['images']['workspace']['export_digest'])])
        self.assertEqual(graph['layers'][0]['mediaType'], GZIP)

    def test_nested_index_proof_closure_retains_all_original_objects(self):
        value = fixture()
        descriptor = add_proof(value, 'workspace', nested=True)
        original = value['files'][blob(descriptor['digest'])]
        _, emitted = self.convert(value)
        self.assertEqual(emitted[blob(descriptor['digest'])], original)
        self.assertIn(descriptor, json.loads(emitted['index.json'])['manifests'])

    def test_upstream_raw_and_converted_gzip_same_digest_deduplicate_in_both_orders(self):
        for raw_first in (False, True):
            with self.subTest(raw_first=raw_first):
                value = fixture(upstream_raw=True)
                raw_name = blob(value['graphs']['redis']['layers'][0]['digest'])
                files = value['files']
                ordered = {raw_name: files[raw_name], **{name: data for name, data in files.items() if name != raw_name}} if raw_first else {**{name: data for name, data in files.items() if name != raw_name}, raw_name: files[raw_name]}
                result, emitted = self.convert(value, files=ordered)
                self.assertEqual(emitted[raw_name], files[raw_name])
                graph = json.loads(emitted[blob(result['images']['workspace']['export_digest'])])
                self.assertEqual(graph['layers'][0]['digest'], digest(files[raw_name]))
                self.assertEqual(graph['layers'][0]['mediaType'], RAW)

    def test_central_xz_and_legacy_gzip_inputs_have_the_same_result(self):
        value = fixture()
        gzip_result, gzip_files = self.convert(value)
        xz_result, xz_files = self.convert(value, 'xz')
        self.assertEqual(gzip_result, xz_result)
        self.assertEqual(gzip_files, xz_files)

    def test_wrong_index_digest_binding_is_rejected(self):
        value = fixture()
        replacement = copy.deepcopy(next(row for row in value['index']['manifests'] if row.get('annotations', {}).get('io.containerd.image.name') == 'docker.io/' + value['records']['redis']['ref']))
        original = value['index']['manifests'][0]
        original.update({name: replacement[name] for name in ('digest', 'size', 'mediaType')})
        sync_metadata(value)
        with self.assertRaises(ValueError):
            self.convert(value)

    def test_wrong_index_descriptor_size_is_rejected(self):
        value = fixture()
        value['index']['manifests'][0]['size'] += 1
        sync_metadata(value)
        with self.assertRaises(ValueError):
            self.convert(value)

    def test_wrong_docker_config_and_layers_bindings_are_rejected(self):
        for field in ('Config', 'Layers'):
            with self.subTest(field=field):
                value = fixture()
                value['docker'][0][field] = copy.deepcopy(value['docker'][-1][field])
                sync_metadata(value)
                with self.assertRaises(ValueError):
                    self.convert(value)

    def test_converted_and_shared_layer_wrong_diff_ids_are_rejected(self):
        for shared in (False, True):
            with self.subTest(shared_upstream=shared):
                value = fixture(shared_upstream=shared)
                replace_role_config(value, 'desktop', lambda config: config['rootfs']['diff_ids'].__setitem__(0, 'sha256:' + '0' * 64))
                with self.assertRaises(ValueError):
                    self.convert(value)

    def test_missing_proof_reachable_blob_is_rejected(self):
        value = fixture()
        descriptor = add_proof(value, 'workspace', nested=True)
        tree = json.loads(value['files'][blob(descriptor['digest'])])
        del value['files'][blob(tree['manifests'][0]['digest'])]
        with self.assertRaises(ValueError):
            self.convert(value)

    def test_source_blob_corruption_is_rejected(self):
        value = fixture()
        name = blob(value['graphs']['workspace']['layers'][0]['digest'])
        value['files'][name] = b'not the described gzip bytes'
        with self.assertRaises((ValueError, OSError, EOFError)):
            self.convert(value)

    def test_truncated_outer_capsule_is_rejected(self):
        value = fixture()
        data = compress(value)
        with self.assertRaises((ValueError, OSError, EOFError)):
            m.transform(io.BytesIO(data[:-8]), io.BytesIO(), value['records'])

    def test_declared_large_blob_is_rejected_before_any_body_read(self):
        entry = header('blobs/sha256/' + '0' * 64, 1 << 60)
        data = gzip.compress(entry.tobuf(format=tarfile.GNU_FORMAT) + b'\0' * 1024, mtime=0)
        with patch.object(tarfile.TarFile, 'extractfile', side_effect=forbidden):
            with self.assertRaises(ValueError):
                m.transform(io.BytesIO(data), io.BytesIO(), fixture()['records'])


    def test_same_manifest_digest_in_local_and_upstream_roles_retains_upstream_root(self):
        value = fixture()
        upstream = copy.deepcopy(value['records']['redis'])
        old_ref = value['records']['workspace']['ref']
        reference = 'ai-phone-bundle/workspace:sha-' + upstream['export_digest'][7:]
        value['records']['workspace'] = {**upstream, 'ref': reference}
        value['graphs']['workspace'] = copy.deepcopy(value['graphs']['redis'])
        for item in value['index']['manifests']:
            if item.get('annotations', {}).get('io.containerd.image.name') == 'docker.io/' + old_ref:
                item.update(digest=upstream['export_digest'], size=len(value['files'][blob(upstream['export_digest'])]))
                item['annotations'] = {'io.containerd.image.name': 'docker.io/' + reference,
                                       'org.opencontainers.image.ref.name': reference.split(':', 1)[1]}
        # The role root must preserve itself even when no proof independently reaches it.
        value['index']['manifests'] = [item for item in value['index']['manifests']
            if item.get('annotations', {}).get('io.containerd.manifest.subject') != upstream['export_digest']]
        for row in value['docker']:
            if row['RepoTags'] == [old_ref]:
                row.update(Config=blob(upstream['config_digest']), RepoTags=[reference],
                           Layers=[blob(layer['digest']) for layer in value['graphs']['redis']['layers']])
        sync_metadata(value)
        result, emitted = self.convert(value)
        self.assertEqual(result['images']['redis'], upstream)
        for identity in (upstream['export_digest'], upstream['config_digest']):
            self.assertEqual(emitted[blob(identity)], value['files'][blob(identity)])
        graph = json.loads(emitted[blob(result['images']['workspace']['export_digest'])])
        self.assertEqual(graph, value['graphs']['redis'])
        for layer in graph['layers']:
            name = blob(layer['digest'])
            self.assertEqual(layer['mediaType'], GZIP)
            self.assertEqual(emitted[name], value['files'][name])
            self.assertIn(name, result['preserved_object_hashes'])



    def test_third_pass_larger_retained_header_is_rejected_before_body_read(self):
        value = fixture()
        source = io.BytesIO(compress(value))
        name = blob(value['records']['postgres']['config_digest'])
        changed = dict(value['files'])
        changed[name] += b' ' * 32
        changed_bytes = compress(value, files=changed)
        calls = {'pass': 0}
        original_read_tar = m.read_tar
        original_extract = tarfile.TarFile.extractfile

        @contextlib.contextmanager
        def mutated_read_tar(current, *args, **kwargs):
            calls['pass'] += 1
            if calls['pass'] == 3:
                current.seek(0)
                current.truncate()
                current.write(changed_bytes)
                current.seek(0)
            with original_read_tar(current, *args, **kwargs) as archive:
                yield archive

        def observed_extract(archive, member, *args, **kwargs):
            member_name = member.name if isinstance(member, tarfile.TarInfo) else member
            if calls['pass'] == 3 and member_name == name:
                raise AssertionError('changed retained header must be rejected before reading its body')
            return original_extract(archive, member, *args, **kwargs)

        with patch.object(m, 'read_tar', new=mutated_read_tar), patch.object(tarfile.TarFile, 'extractfile', new=observed_extract):
            with self.assertRaisesRegex(ValueError, 'changed|size'):
                m.transform(source, io.BytesIO(), value['records'])
        self.assertEqual(calls['pass'], 3)

    def test_third_pass_inner_gzip_growth_stops_at_verified_raw_size_plus_one(self):
        value = fixture()
        source = io.BytesIO(compress(value))
        name = blob(value['graphs']['workspace']['layers'][0]['digest'])
        expected_size = len(value['plain'][name])
        expanded = gzip.compress(b'\0' * (expected_size + 65536), mtime=0)
        self.assertLess(len(expanded), len(value['files'][name]))
        changed = dict(value['files'])
        changed[name] = expanded + b'\0' * (len(value['files'][name]) - len(expanded))
        self.assertEqual(len(changed[name]), len(value['files'][name]))
        changed_bytes = compress(value, files=changed)
        calls = {'pass': 0, 'plain_bytes': 0}
        original_read_tar = m.read_tar
        original_gzip_read = gzip.GzipFile.read

        @contextlib.contextmanager
        def mutated_read_tar(current, *args, **kwargs):
            calls['pass'] += 1
            if calls['pass'] == 3:
                current.seek(0)
                current.truncate()
                current.write(changed_bytes)
                current.seek(0)
            with original_read_tar(current, *args, **kwargs) as archive:
                yield archive

        def observed_gzip_read(stream, size=-1):
            data = original_gzip_read(stream, size)
            if calls['pass'] == 3:
                calls['plain_bytes'] += len(data)
            return data

        with patch.object(m, 'read_tar', new=mutated_read_tar), patch.object(gzip.GzipFile, 'read', new=observed_gzip_read):
            with self.assertRaises(ValueError):
                m.transform(source, io.BytesIO(), value['records'])
        self.assertEqual(calls['pass'], 3)
        self.assertGreater(calls['plain_bytes'], 0)
        self.assertLessEqual(calls['plain_bytes'], expected_size + 1)


if __name__ == '__main__':
    unittest.main()
