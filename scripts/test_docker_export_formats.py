"""Regression contracts for actual engine formats; no engine used by these tests."""
import copy
import importlib.util
import io
import json
from pathlib import Path
import sys
import unittest

import test_oci_layer_transform as fixtures
import oci_layer_transform as owner
DOCKER_IMAGE = 'application/vnd.docker.distribution.manifest.v2+json'
DOCKER_GZIP = 'application/vnd.docker.image.rootfs.diff.tar.gzip'


def docker_fixture():
    value = fixtures.fixture()
    value['index']['manifests'] = []
    for role in fixtures.ROLES:
        record = value['records'][role]
        graph = copy.deepcopy(value['graphs'][role])
        graph['mediaType'] = DOCKER_IMAGE
        graph['config']['mediaType'] = 'application/vnd.docker.container.image.v1+json'
        for layer in graph['layers']:
            layer['mediaType'] = DOCKER_GZIP
        descriptor = fixtures.add_blob(value, fixtures.encoded(graph), DOCKER_IMAGE)
        ref = 'ai-phone-bundle/' + role + ':sha-' + descriptor['digest'][7:]
        record.update(ref=ref, export_digest=descriptor['digest'])
        value['graphs'][role] = graph
        value['docker'][fixtures.ROLES.index(role)]['RepoTags'] = [ref]
        value['index']['manifests'].append({**descriptor, 'annotations': {
            'io.containerd.image.name': 'docker.io/' + ref,
            'org.opencontainers.image.ref.name': ref.split(':', 1)[1]}})
    fixtures.sync_metadata(value)
    return value


class DockerExportFormats(unittest.TestCase):
    def test_docker_schema2_local_conversion_preserves_configs_diffids_and_upstream(self):
        value = docker_fixture()
        original = copy.deepcopy(value['records'])
        source = io.BytesIO(fixtures.gzip.compress(fixtures.tar_bytes(value['files'])))
        destination = io.BytesIO()
        result = owner.transform(source, destination, value['records'])
        _, files = fixtures.output_files(destination.getvalue())
        for role, record in result['images'].items():
            self.assertEqual(record['config_digest'], original[role]['config_digest'])
            self.assertEqual(files[fixtures.blob(record['config_digest'])], value['files'][fixtures.blob(record['config_digest'])])
            if role in ('workspace', 'desktop'):
                self.assertNotEqual(record['ref'], original[role]['ref'])
                graph = json.loads(files[fixtures.blob(record['export_digest'])])
                self.assertEqual(graph['mediaType'], fixtures.IMAGE)
                self.assertTrue(all(layer['mediaType'] == fixtures.RAW for layer in graph['layers']))
            else:
                self.assertEqual(record, original[role])
                self.assertEqual(files[fixtures.blob(record['export_digest'])], value['files'][fixtures.blob(record['export_digest'])])
        self.assertEqual(value['records'], original)


if __name__ == '__main__':
    unittest.main()
