"""Snapshot the locked production dependency graph without changing Node resolution."""
import json
import os
from pathlib import Path
import platform
import re

MAX_PACKAGES = 1024
NAME = re.compile(r'(?:@[A-Za-z0-9._-]+/)?[A-Za-z0-9._-]+')


def package_data(folder):
    path = folder / 'package.json'
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 1024**2:
        raise ValueError('Invalid dependency package metadata')
    value = json.loads(path.read_text())
    if not isinstance(value, dict):
        raise ValueError('Invalid dependency package metadata')
    return value


def mapping(value, field):
    data = value.get(field, {})
    if not isinstance(data, dict):
        raise ValueError('Invalid dependency declarations: ' + field)
    return data


def dependencies(value):
    required = dict(mapping(value, 'dependencies'))
    optional = dict(mapping(value, 'optionalDependencies'))
    peers = mapping(value, 'peerDependenciesMeta')
    for name, version in mapping(value, 'peerDependencies').items():
        declaration = peers.get(name, {})
        if not isinstance(declaration, dict):
            raise ValueError('Invalid peer dependency declaration')
        target = optional if declaration.get('optional') else required
        target[name] = version
    required.update(optional)
    for name, version in required.items():
        if NAME.fullmatch(name) is None or not isinstance(version, str):
            raise ValueError('Invalid dependency name or version')
    return required, set(optional)


def matches(values, actual):
    if not isinstance(values, list) or any(not isinstance(v, str) for v in values):
        raise ValueError('Invalid dependency platform declaration')
    positive = [v for v in values if not v.startswith('!')]
    return '!' + actual not in values and (not positive or actual in positive)


def platform_tags():
    system = {'Darwin': 'darwin', 'Windows': 'win32', 'Linux': 'linux'}.get(platform.system())
    arch = {'arm64': 'arm64', 'aarch64': 'arm64', 'AMD64': 'x64', 'x86_64': 'x64'}.get(platform.machine())
    if not system or not arch:
        raise ValueError('Unsupported JavaScript runtime host')
    result = {'os': system, 'cpu': arch}
    if system == 'linux':
        result['libc'] = platform.libc_ver()[0] or 'unknown'
    return result


def expected_name(name, version):
    if not version.startswith('npm:'):
        return name
    alias = version[4:]
    return alias.rsplit('@', 1)[0] if '@' in alias[1:] else alias


class DependencyTree:
    def __init__(self, root, source=None):
        self.root = Path(root).resolve()
        self.source = Path(source).resolve() if source is not None else self.root
        self.packages = {}
        self.aliases = {}
        self.tags = platform_tags()

    def package_source(self, path):
        relative = path.relative_to(self.root)
        return path if 'node_modules' in relative.parts else self.source / relative

    def resolve(self, start, name, version, optional):
        for base in (start, *start.parents):
            if not base.is_relative_to(self.root):
                break
            if base.name == 'node_modules':
                continue
            logical = base / 'node_modules' / name
            if logical.exists():
                return self.admit(logical, name, version, optional)
        if not optional:
            raise ValueError('Missing locked dependency: ' + name)
        return None

    def admit(self, logical, name, version, optional):
        target = logical.resolve()
        if not target.is_relative_to(self.root):
            raise ValueError('Dependency escapes the build workspace')
        meta = package_data(self.package_source(target))
        if meta.get('name') != expected_name(name, version) or not isinstance(meta.get('version'), str):
            raise ValueError('Dependency identity differs from its declaration')
        if not all(matches(meta.get(key, []), actual) for key, actual in self.tags.items()):
            if optional:
                return None
            raise ValueError('Required dependency does not match this host')
        if logical != target:
            self.aliases[logical.relative_to(self.root).as_posix()] = target.relative_to(self.root).as_posix()
        return target

    def add(self, path):
        if path is None or path in self.packages:
            return
        if len(self.packages) >= MAX_PACKAGES:
            raise ValueError('Dependency graph exceeds its package budget')
        meta = package_data(self.package_source(path))
        self.packages[path] = meta
        required, optional = dependencies(meta)
        for name, version in sorted(required.items()):
            self.add(self.resolve(path, name, version, name in optional))

    def application(self, name):
        folder = self.root / name
        required, optional = dependencies(package_data(self.source / name))
        for key, version in required.items():
            self.add(self.resolve(folder, key, version, key in optional))


def collect(root, applications, *, source=None):
    tree = DependencyTree(root, source)
    for name in sorted(set(applications)):
        tree.application(name)
    return tree


def package_files(folder):
    for base, dirs, files in os.walk(folder):
        dirs[:] = sorted(name for name in dirs if name != 'node_modules')
        if any((Path(base) / name).is_symlink() for name in dirs):
            raise ValueError('Dependency contains a directory alias outside its graph')
        for name in sorted(files):
            source = Path(base) / name
            if source.suffix == '.map' or source.name == '.env.example':
                continue
            if not source.resolve().is_relative_to(folder) or not source.is_file():
                raise ValueError('Dependency resource is not contained in its package')
            yield source
