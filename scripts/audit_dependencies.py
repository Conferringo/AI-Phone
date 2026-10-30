"""Audit locked dependencies and retain target reachability; never blanket-ignore an advisory."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
from native_backports import verified_backports

ROOT = Path(__file__).resolve().parent.parent
TARGETS = ('aarch64-apple-darwin', 'aarch64-unknown-linux-gnu', 'x86_64-unknown-linux-gnu', 'x86_64-pc-windows-msvc', 'aarch64-linux-android')


def findings(report, reachable):
    rows = []
    groups = {'vulnerability': report.get('vulnerabilities', {}).get('list', []), **report.get('warnings', {})}
    for category, items in groups.items():
        for item in items:
            package, advisory = item['package'], item['advisory']
            key = (package['name'], package['version'])
            rows.append({'package': '@'.join(key), 'advisory': advisory['id'], 'category': category,
                         'title': advisory['title'], 'url': advisory['url'],
                         'active_targets': [target for target, graph in reachable.items() if key in graph],
                         'blocking_when_active': category in ('vulnerability', 'unsound')})
    return rows


def audit(root=ROOT):
    rust = {}
    backports = verified_backports(root)
    for name, manifest in [('server', 'Cargo.toml'), ('client', 'apps/client/src-tauri/Cargo.toml')]:
        lock = Path(manifest).with_name('Cargo.lock')
        run = subprocess.run(['cargo', 'audit', '--file', str(lock), '--json'], cwd=root, capture_output=True, text=True)
        if run.returncode not in (0, 1):
            raise RuntimeError(f'{name}: cargo audit did not finish')
        report = json.loads(run.stdout)
        if 'vulnerabilities' not in report or not report.get('database', {}).get('last-commit'):
            raise RuntimeError(f'{name}: missing advisory database or result')
        reachable = {}
        for target in TARGETS:
            tree = subprocess.check_output(['cargo', 'tree', '--manifest-path', manifest, '--locked',
                '--target', target, '--prefix', 'none', '--format', '{p}'], cwd=root, text=True)
            reachable[target] = {(parts[0], parts[1][1:]) for line in tree.splitlines()
                if len(parts := line.split()) >= 2 and parts[1].startswith('v')}
        rows = findings(report, reachable)
        # cargo-audit excludes path dependencies. Keep vendored advisories visible
        # and require the exact source hash check above before accepting mitigation.
        if name == 'client':
            for (package, advisory), mitigation in backports.items():
                if not any(row['package'] == package and row['advisory'] == advisory for row in rows):
                    key = tuple(package.rsplit('@', 1))
                    rows.append({'package': package, 'advisory': advisory, 'category': 'unsound',
                        'title': 'Verified vendored upstream backport; see source manifest',
                        'url': 'https://rustsec.org/advisories/'+advisory+'.html',
                        'active_targets': [target for target, graph in reachable.items() if key in graph],
                        'blocking_when_active': True})
        for row in rows:
            mitigation = backports.get((row['package'], row['advisory'])) if name == 'client' else None
            if mitigation:
                row['mitigation'] = mitigation
                row['blocking_when_active'] = False
        rust[name] = {'lock_sha256': hashlib.sha256((root/lock).read_bytes()).hexdigest(),
                      'database': report['database'], 'findings': rows}
    js = subprocess.run([str(root/'scripts/bun'), 'audit', '--json'], cwd=root, capture_output=True, text=True)
    if js.returncode not in (0, 1):
        raise RuntimeError('JavaScript audit did not finish')
    javascript = json.loads(js.stdout)
    blocked = {target: [f'{name}:{row["advisory"]}' for name, report in rust.items()
        for row in report['findings'] if row['blocking_when_active'] and target in row['active_targets']]
        for target in TARGETS}
    return {'format': 1, 'scope': 'Locked dependency advisories and selected build graphs; not platform runtime acceptance',
            'rust': rust, 'javascript': javascript, 'javascript_passed': js.returncode == 0,
            'blocked_targets': blocked, 'blanket_ignores': []}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--target', choices=TARGETS, help='Fail on active vulnerability/unsound findings for this target')
    args = parser.parse_args()
    result = audit()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps({'javascript_passed': result['javascript_passed'], 'blocked_targets': result['blocked_targets']}))
    if not result['javascript_passed'] or (args.target and result['blocked_targets'][args.target]):
        raise SystemExit(1)
