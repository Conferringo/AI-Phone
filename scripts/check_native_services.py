"""Execute native component checks only against generated inputs and empty state."""
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
from runtime_verification import isolated_environment


def version_receipt(program, name, version):
    program = Path(program).resolve()
    with tempfile.TemporaryDirectory(prefix='aiphone-service-version-') as folder:
        root = Path(folder)
        environment = isolated_environment(root)
        environment.update(HOME=str(root), USERPROFILE=str(root))
        result = subprocess.run([program, '--version'], cwd=root, env=environment, capture_output=True, timeout=10)
        if result.returncode or result.stdout.decode().strip() != f'{name} {version}' or result.stderr.strip():
            raise ValueError(name + ' version check failed')
        if any(root.iterdir()):
            raise ValueError(name + ' version command modified state')
    return {'program': name, 'version': version, 'state_created': False}


def vision_receipt(program, fixture, *, python=None):
    png = Path(fixture).read_bytes()
    expected = struct.unpack('>II', png[16:24])
    with tempfile.TemporaryDirectory(prefix='aiphone-service-vision-') as folder:
        root = Path(folder)
        environment = isolated_environment(root)
        if python is not None: environment['AI_PHONE_PYTHON'] = str(python)
        result = subprocess.run([Path(program).resolve()], input=png, cwd=root,
                                env=environment, capture_output=True, timeout=10)
        if result.returncode:
            raise ValueError('Bundled OCR could not recognize the generated fixture')
        value = json.loads(result.stdout)
        if (value['width'], value['height']) != expected or len(value['grid']) != 2048:
            raise ValueError('Bundled OCR returned an invalid image/grid size')
        text = ''.join(row['text'] for row in value['regions']).replace(' ', '')
        if text != '7319':
            raise ValueError('Bundled OCR did not recognize the generated digits')
    return {'fixture_text': text, 'width': expected[0], 'height': expected[1], 'grid': 2048}


def verify(root, *, tool_runtime=None):
    from prebuilt_services import load_manifest, binary_path, FIXTURE
    root = Path(root).resolve()
    value = load_manifest(root)
    checks = {role: version_receipt(root / binary_path(role), 'ai-phone-' + role, value['versions'][role])
              for role in ('assistant', 'runner')}
    python = None
    if value.get('vision') == 'linux-tesseract-bundled':
        from installer_tools import verify as verify_tools
        if tool_runtime is None: raise ValueError('Bundled OCR verification requires portable installer tools')
        tools = verify_tools(tool_runtime); info = tools['tools']['python']
        if not info['version'].startswith('3.13.'): raise ValueError('Bundled OCR requires Python 3.13')
        python = Path(tool_runtime) / info['executable']
    checks['vision'] = vision_receipt(root / binary_path('vision'), root / FIXTURE, python=python)
    return checks


def require_test_database(container, port):
    if not container.startswith('aiphone-native-') or type(port) is not int or not 1024 <= port <= 65535 or port == 55482:
        raise ValueError('Native checks require a dedicated disposable database')
    output = subprocess.check_output(['docker', 'inspect', '-f', '{{json .Config.Labels}}\n{{json .HostConfig.PortBindings}}', container], text=True)
    labels, bindings = map(json.loads, output.splitlines())
    if not isinstance(labels, dict) or labels.get('dev.aiphone.test') != 'native-runtime':
        raise ValueError('Native database fixture label is missing')
    if not isinstance(bindings, dict) or bindings.get('5432/tcp') != [{'HostIp': '127.0.0.1', 'HostPort': str(port)}]:
        raise ValueError('Native database fixture must use the declared loopback port')


def runtime_sql(container, credentials, sql):
    environment = {**os.environ, 'PGPASSWORD': credentials['runtime']['password']}
    return subprocess.run(['docker', 'exec', '-e', 'PGPASSWORD', container, 'psql', '-h', '127.0.0.1', '-X',
                           '-v', 'ON_ERROR_STOP=1', '-U', credentials['runtime']['username'], '-d', 'ai_phone', '-tA', '-c', sql],
                          env=environment, capture_output=True, text=True, timeout=10)


def database_receipt(root, container, port):
    from prebuilt_services import service_command
    from database import prepare, credentials, RUNTIME
    require_test_database(container, port)
    program = service_command(root, 'assistant')[0]
    with tempfile.TemporaryDirectory(prefix='aiphone-native-db-') as folder:
        fixture = Path(folder)
        environment = isolated_environment(fixture)
        environment['AI_PHONE_PG_PORT'] = str(port)
        (fixture / '.env').write_text('POSTGRES_DB=ai_phone\nPOSTGRES_USER=ai_phone\nPOSTGRES_PASSWORD=isolated-native-fixture-password\n')
        prepare(fixture, container)
        subprocess.run([program, '--migrate'], cwd=fixture, env=environment, capture_output=True, check=True, timeout=30)
        result = subprocess.run([program, '--database-check'], cwd=fixture, env=environment, capture_output=True, check=True, timeout=15)
        status = json.loads(result.stdout)
        if status != {'runtime': RUNTIME, 'privileged': False, 'ddl': False}:
            raise ValueError('Prebuilt runtime did not use the expected restricted database role')
        login = credentials(fixture)
        migrations = runtime_sql(container, login, 'SELECT count(*) FROM _sqlx_migrations WHERE success;')
        if migrations.returncode or int(migrations.stdout.strip()) < 1:
            raise ValueError('Prebuilt schema migrations were not readable by the runtime')
        ddl = runtime_sql(container, login, 'CREATE TABLE native_probe_forbidden(id int);')
        if ddl.returncode == 0 or 'permission denied' not in ddl.stderr.lower():
            raise ValueError('Prebuilt runtime DDL restriction was not established')
        return {'runtime': RUNTIME, 'migrations': int(migrations.stdout.strip()), 'ddl_refused': True, 'fixture_only': True}
