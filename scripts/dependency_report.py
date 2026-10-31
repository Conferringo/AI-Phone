"""Allowlisted CI diagnostics; the audit result owns all blocking decisions."""
import json
from pathlib import Path
import re

PACKAGE = r'(?:@[a-z0-9._-]+/)?[a-z0-9._-]+(?:@[0-9][a-zA-Z0-9.+_-]*)?'
ADVISORY = r'(?:RUSTSEC-\d{4}-\d{4}|GHSA-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}|NPM-\d{1,12})'
CATEGORIES = {'vulnerability', 'unsound', 'unmaintained', 'yanked', 'notice'}
SEVERITIES = {'info', 'low', 'moderate', 'high', 'critical'}


def identifier(value, pattern):
    if not isinstance(value, str) or len(value) > 256 or re.fullmatch(pattern, value) is None:
        return 'unknown'
    return value


def js_advisory(row):
    link = row.get('url', '')
    match = re.fullmatch(r'https://github\.com/advisories/(GHSA-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4})(?:[?#].*)?', link) if isinstance(link, str) else None
    if match:
        return match[1]
    number = row.get('id')
    return 'NPM-' + str(number) if type(number) is int and 0 < number < 10**12 else 'unknown'


def javascript_findings(value):
    if not isinstance(value, dict):
        raise ValueError('Invalid JavaScript audit diagnostics')
    rows = []
    for package, entries in value.items():
        if not isinstance(entries, list) or any(not isinstance(entry, dict) for entry in entries):
            raise ValueError('Invalid JavaScript audit diagnostics')
        for entry in entries:
            severity = entry.get('severity')
            rows.append({'package': identifier(package, PACKAGE), 'advisory': js_advisory(entry),
                         'severity': severity if isinstance(severity, str) and severity in SEVERITIES else 'unknown'})
    return rows


def rust_finding(row, targets):
    category = row.get('category')
    value = {'package': identifier(row.get('package'), PACKAGE),
             'advisory': identifier(row.get('advisory'), ADVISORY),
             'category': category if isinstance(category, str) and category in CATEGORIES else 'unknown',
             'active_targets': [target for target in targets if target in row['active_targets']],
             'blocking_when_active': row['blocking_when_active']}
    if type(value['blocking_when_active']) is not bool:
        raise ValueError('Invalid dependency audit decision')
    if 'mitigation' in row:
        value['mitigation'] = 'verified-upstream-backport' if row['mitigation'].get('type') == 'verified-upstream-backport' else 'unknown'
    return value


def sanitized_report(result, targets):
    rust = {}
    for name in ('server', 'client'):
        if name not in result['rust']:
            continue
        report = result['rust'][name]
        rust[name] = {'lock_sha256': identifier(report.get('lock_sha256'), r'[a-f0-9]{64}'),
                      'database_commit': identifier(report.get('database', {}).get('last-commit'), r'[a-f0-9]{40,64}'),
                      'findings': [rust_finding(row, targets) for row in report['findings']]}
    blocked = {target: [identifier(value, r'(?:server|client):' + ADVISORY)
                       for value in result['blocked_targets'][target]] for target in targets}
    if type(result['javascript_passed']) is not bool:
        raise ValueError('Invalid dependency audit decision')
    return {'format': 2, 'completed': True, 'javascript_passed': result['javascript_passed'],
            'blocked_targets': blocked, 'rust': rust,
            'javascript': javascript_findings(result['javascript']), 'blanket_ignores': []}


def report_lines(report):
    yield json.dumps({key: report[key] for key in ('javascript_passed', 'blocked_targets')})
    rows = [('javascript', row, row['severity']) for row in report['javascript']]
    rows.extend((name, row, row['category']) for name, part in report['rust'].items() for row in part['findings'])
    for name, row, category in rows[:40]:
        yield f"{name}: {row['package']} [{category}] {row['advisory']}"
    if len(rows) > 40:
        yield f'{len(rows) - 40} further findings are retained in the audit artifact.'


def write_report(path, report):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
