"""Validate the coverage inventory and one JSON E2E table without executing ROI."""
import argparse
import collections
import json
import math
import re
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]
import sys
sys.path.insert(0, str(ROOT / 'test-specs'))
from case_table import read_table

ERRORS = {'#NULL!', '#DIV/0!', '#VALUE!', '#REF!', '#NAME?', '#NUM!', '#N/A', '#GETTING_DATA', '#SPILL!', '#CALC!'}
KINDS = {'documented_example', 'documented_derivation', 'excel_observation', 'unresolved'}


def keys(obj, required, optional=()):
    assert isinstance(obj, dict), obj
    assert set(required) <= obj.keys(), ('missing fields', set(required) - obj.keys())
    assert obj.keys() <= set(required) | set(optional), ('unknown fields', obj.keys() - set(required) - set(optional))


def urls(items):
    assert isinstance(items, list) and items
    assert len(items) == len(set(items))
    for url in items:
        parsed = urlparse(url)
        assert parsed.scheme == 'https' and parsed.hostname in {'support.microsoft.com', 'learn.microsoft.com', 'techcommunity.microsoft.com'}, url


def typed_value(value, array=False):
    keys(value, ['type'], ['value'])
    kind, payload = value['type'], value.get('value')
    if kind == 'number':
        assert type(payload) in (int, float) and math.isfinite(payload)
    elif kind == 'text':
        assert isinstance(payload, str)
    elif kind == 'boolean':
        assert isinstance(payload, bool)
    elif kind == 'error':
        assert payload in ERRORS
    elif kind == 'utf16':
        assert isinstance(payload, list) and all(type(u) is int and 0 <= u <= 65535 for u in payload)
    elif kind == 'array':
        assert array and isinstance(payload, list) and payload
        assert all(isinstance(row, list) and row for row in payload)
        assert len({len(row) for row in payload}) == 1
        for row in payload:
            for cell in row:
                typed_value(cell)
    else:
        assert kind == 'blank', kind


def address(item, sheets):
    keys(item, ['sheet', 'address'])
    assert type(item['sheet']) is int and 0 <= item['sheet'] < len(sheets)
    match = re.fullmatch(r'([A-Z]{1,3})([1-9][0-9]*)', item['address'])
    assert match, item
    letters, row = match.groups()
    col = 0
    for letter in letters:
        col = col * 26 + ord(letter) - ord('A') + 1
    assert col <= 16384 and int(row) <= 1048576
    return item['sheet'], int(row), col


def cells(items, sheets):
    assert isinstance(items, list)
    seen = set()
    for cell in items:
        keys(cell, ['sheet', 'address'], ['value', 'formula'])
        at = address({k: cell[k] for k in ['sheet', 'address']}, sheets)
        assert at not in seen, ('duplicate coordinate', at)
        seen.add(at)
        assert ('value' in cell) != ('formula' in cell)
        if 'formula' in cell:
            assert isinstance(cell['formula'], str) and cell['formula'].strip()
        else:
            typed_value(cell['value'])


def names(items, sheets):
    assert isinstance(items, list)
    seen = set()
    for name in items:
        keys(name, ['name', 'formula', 'sheet_scope'])
        assert isinstance(name['name'], str) and name['name']
        assert isinstance(name['formula'], str) and name['formula']
        scope = name['sheet_scope']
        assert scope is None or type(scope) is int and 0 <= scope < len(sheets)
        key = (name['name'].casefold(), scope)
        assert key not in seen, key
        seen.add(key)


def settings(obj, step=False):
    keys(obj, [], ['currency', 'context', 'text_policy', 'array_result_policy'] + ([] if step else ['date_system']))
    if 'date_system' in obj:
        assert obj['date_system'] in {'1900', '1904'}
    if 'currency' in obj:
        assert obj['currency'] == 'USD'
    if 'text_policy' in obj:
        assert obj['text_policy'] in {'excel_v1', 'excel_v2'}
    if 'array_result_policy' in obj:
        assert obj['array_result_policy'] in {'scalar_intersection', 'dynamic_spill'}
    if 'context' in obj:
        context = obj['context']
        keys(context, [], ['as_of_time', 'randomness_seed', 'utc_offset_seconds', 'date_order'])
        if 'as_of_time' in context:
            instant = context['as_of_time']
            keys(instant, ['secs_since_epoch', 'nanos_since_epoch'])
            assert type(instant['secs_since_epoch']) is int and 0 <= instant['secs_since_epoch'] < 2**64
            assert type(instant['nanos_since_epoch']) is int and 0 <= instant['nanos_since_epoch'] < 10**9
        if 'randomness_seed' in context:
            assert type(context['randomness_seed']) is int and 0 <= context['randomness_seed'] < 2**64
        if 'utc_offset_seconds' in context:
            assert type(context['utc_offset_seconds']) is int and -(2**31) <= context['utc_offset_seconds'] < 2**31
        if 'date_order' in context:
            assert context['date_order'] in {'MDY', 'DMY'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write-report', action='store_true')
    args = parser.parse_args()
    inventory = json.loads((ROOT / 'test-specs/inventory.json').read_text())
    features = inventory['features']
    assert len({f['id'] for f in features}) == len(features)
    obligations = {o['id']: (f, o) for f in features for o in f['obligations']}
    assert len(obligations) == sum(len(f['obligations']) for f in features)
    registered = {f['name'] for f in features if f['kind'] == 'function' and f['implementation_status'] == 'registered'}
    # Public registry freshness is checked by the Rust runner through ROI APIs.
    for feature in features:
        urls(feature['source_urls'])
        assert feature['scope_status'] in {'in_scope', 'out_of_scope'}
        assert feature['implementation_status'] in {'registered', 'partial', 'unsupported', 'unknown'}
        if 'implementation_scope' in feature:
            scope = feature['implementation_scope']
            assert scope['policies'] and all(isinstance(policy, str) and policy.strip() for policy in scope['policies'])
            assert scope['supported'].strip()
            assert scope['limitations'] and all(isinstance(limit, str) and limit.strip() for limit in scope['limitations'])
        for obligation in feature['obligations']:
            assert obligation['behavior'].strip()
            assert obligation['applicability'] in {'applicable', 'requires_research', 'not_applicable'}
    cases = read_table(ROOT)
    assert len(cases) >= 1000
    assert [c['id'] for c in cases] == list(range(len(cases)))
    assert len({c['name'] for c in cases}) == len(cases)
    rust = (ROOT / 'tests/workbook_e2e.rs').read_text()
    assert rust.count('#[test]') == 1 and '#[ignore' not in rust and 'poirot_test_runner::json::specs::check_table()' in rust
    for source_file in (ROOT / 'tests').rglob('*.rs'):
        content = source_file.read_text()
        resources = re.findall(r'include_str!\("([^\"]+)"\)', content)
        resources += re.findall(r'#\[path = "([^\"]+)"\]', content)
        for relative in resources:
            resource = (source_file.parent / relative).resolve()
            assert resource.is_relative_to(ROOT) and resource.is_file(), ('nonlocal resource', source_file, relative)
    fingerprints, evidence_counts, links, unresolved = set(), collections.Counter(), collections.defaultdict(list), []
    assertions_count = steps_count = 0
    for case in cases:
        keys(case, ['schema_version', 'id', 'name', 'category', 'compatibility', 'text_policy', 'sheets', 'cells', 'names', 'settings', 'steps', 'source_urls', 'rationale'])
        assert case['schema_version'] == 2
        assert re.fullmatch(r'[a-z][a-z0-9_]*', case['name'])
        assert all(isinstance(case[k], str) and case[k].strip() for k in ['category', 'compatibility', 'rationale'])
        assert case['text_policy'] in {'excel_v1', 'excel_v2'}
        urls(case['source_urls'])
        sheets = case['sheets']
        assert sheets and all(isinstance(s, str) and s for s in sheets) and len(set(sheets)) == len(sheets)
        cells(case['cells'], sheets)
        names(case['names'], sheets)
        settings(case['settings'])
        assert any('formula' in c for c in case['cells']) or any('formula' in e for s in case['steps'] for e in s['edits'])
        assert case['steps']
        for step_index, step in enumerate(case['steps']):
            keys(step, ['edits', 'assertions'], ['settings', 'names', 'recompile'])
            steps_count += 1
            cells(step['edits'], sheets)
            if 'settings' in step:
                settings(step['settings'], step=True)
            if 'names' in step:
                names(step['names'], sheets)
            if 'recompile' in step:
                assert type(step['recompile']) is bool
            assert step['assertions']
            for assertion_index, assertion in enumerate(step['assertions']):
                keys(assertion, ['target', 'evidence', 'obligations'], ['expected', 'tolerance'])
                assertions_count += 1
                _, row, col = address(assertion['target'], sheets)
                evidence = assertion['evidence']
                keys(evidence, ['kind', 'source_urls', 'explanation'], ['review', 'source_section', 'observation', 'derivation'])
                assert evidence['kind'] in KINDS and evidence['explanation'].strip()
                for detail in ('review', 'source_section', 'derivation'):
                    if detail in evidence:
                        assert isinstance(evidence[detail], str) and evidence[detail].strip()
                urls(evidence['source_urls'])
                assert set(evidence['source_urls']) <= set(case['source_urls'])
                evidence_counts[evidence['kind']] += 1
                if evidence['kind'] == 'excel_observation':
                    observation = evidence['observation']
                    keys(observation, ['workbook_sha256', 'excel_version', 'settings', 'result_artifact'])
                    assert re.fullmatch(r'[0-9a-fA-F]{64}', observation['workbook_sha256'])
                    assert isinstance(observation['excel_version'], str) and observation['excel_version'].strip()
                    assert isinstance(observation['result_artifact'], str) and observation['result_artifact'].strip()
                    observed = observation['settings']
                    keys(observed, ['date_system', 'locale', 'compatibility_version'])
                    assert observed['date_system'] in {'1900', '1904'}
                    assert isinstance(observed['locale'], str) and observed['locale'].strip()
                    assert type(observed['compatibility_version']) is int and observed['compatibility_version'] in {1, 2}
                if evidence['kind'] == 'unresolved':
                    unresolved.append({'case_id': case['id'], 'step': step_index, 'assertion': assertion_index, 'explanation': evidence['explanation']})
                else:
                    typed_value(assertion['expected'], array=True)
                    if assertion['expected']['type'] == 'array':
                        assert row + len(assertion['expected']['value']) - 1 <= 1048576
                        assert col + len(assertion['expected']['value'][0]) - 1 <= 16384
                if 'tolerance' in assertion:
                    assert type(assertion['tolerance']) in (int, float) and math.isfinite(assertion['tolerance']) and assertion['tolerance'] >= 0
                    expected = assertion['expected']
                    assert expected['type'] == 'number' or expected['type'] == 'array' and all(v['type'] == 'number' for r in expected['value'] for v in r)
                assert assertion['obligations'] and len(set(assertion['obligations'])) == len(assertion['obligations'])
                for obligation in assertion['obligations']:
                    assert obligation in obligations, (case['id'], obligation)
                    assert obligations[obligation][1]['applicability'] != 'not_applicable', (case['id'], obligation)
                    links[obligation].append({'case_id': case['id'], 'step': step_index, 'assertion': assertion_index, 'evidence_kind': evidence['kind']})
        # Ignore explanations/labels while detecting repeated fixtures and oracles.
        fingerprint = {'sheets': sheets, 'cells': case['cells'], 'names': case['names'], 'settings': case['settings'], 'text_policy': case['text_policy'], 'steps': [
            {k: v for k, v in step.items() if k != 'assertions'} | {'assertions': [{k: v for k, v in a.items() if k not in {'evidence', 'obligations'}} for a in step['assertions']]}
            for step in case['steps']]}
        fingerprint = json.dumps(fingerprint, sort_keys=True)
        assert fingerprint not in fingerprints, ('duplicate fixture', case['id'])
        fingerprints.add(fingerprint)
    sources = json.loads((ROOT / 'test-specs/sources.json').read_text())
    assert len({s['url'] for s in sources}) == len(sources)
    assert {s['url'] for s in sources} == {u for c in cases for u in c['source_urls']}
    for source in sources:
        assert source['publisher'] == 'Microsoft' and source['applicability'].strip()
        assert re.fullmatch(r'[0-9]{4}-[0-9]{2}-[0-9]{2}', source['verified_on'])
        assert source['spec_ids'] == [c['id'] for c in cases if source['url'] in c['source_urls']]
    rows = []
    for oid, (feature, obligation) in obligations.items():
        supported = [ref for ref in links[oid] if ref['evidence_kind'] != 'unresolved']
        state = 'specified_with_evidence' if supported else 'unresolved' if links[oid] else 'not_applicable' if obligation['applicability'] == 'not_applicable' else 'missing'
        rows.append({'id': oid, 'feature': feature['id'], 'behavior': obligation['behavior'], 'applicability': obligation['applicability'], 'specification_status': state, 'assertions': links[oid]})
    specified_features = {r['feature'] for r in rows if r['specification_status'] == 'specified_with_evidence'}
    missing_functions = sorted(f['name'] for f in features if f['implementation_status'] == 'registered' and f['id'] not in specified_features)
    summary = {'case_count': len(cases), 'evaluation_steps': steps_count, 'assertion_count': assertions_count,
        'source_count': len(sources), 'feature_count': len(features), 'potential_obligation_count': len(rows),
        'obligation_status_counts': dict(collections.Counter(r['specification_status'] for r in rows)),
        'evidence_counts': dict(evidence_counts), 'registered_function_count': len(registered),
        'registered_functions_with_specifications': len(registered) - len(missing_functions),
        'registered_functions_without_specifications': missing_functions,
        'implementation_status_counts': dict(collections.Counter(f['implementation_status'] for f in features)),
        'multi_step_cases': sum(len(c['steps']) > 1 for c in cases), 'named_cases': sum(bool(c['names']) for c in cases),
        'date1904_cases': sum(c['settings'].get('date_system') == '1904' for c in cases),
        'array_assertions': sum(a.get('expected', {}).get('type') == 'array' for c in cases for s in c['steps'] for a in s['assertions']),
        'runtime_tests': 'not_run', 'passing_behavior': 'unverified'}
    report = {'summary': summary,
        'interpretation': 'Specifications describe examples of obligations, not exhaustive function contracts or passing ROI behavior. Requires-research facets remain open even when they have examples.',
        'obligations': rows, 'unresolved_expectations': unresolved,
        'scoped_capabilities': [{'id': f['id'], 'name': f['name'], 'implementation_status': f['implementation_status'],
            'implementation_scope': f['implementation_scope'], 'source_urls': f['source_urls']}
            for f in features if 'implementation_scope' in f],
        'unsupported_features': [{'id': f['id'], 'name': f['name'], 'source_urls': f['source_urls']} for f in features if f['implementation_status'] == 'unsupported']}
    if args.write_report:
        (ROOT / 'test-specs/coverage.json').write_text(json.dumps(report, indent=2) + '\n')
        counts = summary['obligation_status_counts']
        md = '# Excel E2E coverage inventory\n\n'
        md += f"The single table contains {len(cases):,} cases, {steps_count:,} evaluation steps and {assertions_count:,} assertions citing {len(sources)} Microsoft sources. {summary['registered_functions_with_specifications']} of {len(registered)} registered function contracts have source-backed specifications. This is specification breadth, not verified passing coverage.\n\n"
        md += f"Of {len(rows):,} potential obligations, {counts.get('specified_with_evidence', 0)} have documented assertion examples, {counts.get('missing', 0)} have no case, {counts.get('unresolved', 0)} have unresolved expectations, and {counts.get('not_applicable', 0)} are not applicable. Generic facets marked requires_research are not considered exhaustive merely because examples exist.\n\n"
        md += f"Workbook coverage includes {summary['multi_step_cases']} multistep cases, {summary['named_cases']} named fixtures, {summary['date1904_cases']} fixtures in the 1904 date system, and {summary['array_assertions']} complete worksheet-footprint assertions.\n\n"
        md += 'Evidence: ' + ', '.join(f'{n:,} {kind.replace("_", " ")}' for kind, n in sorted(evidence_counts.items())) + '. No ROI evaluator tests or Excel observations were run during authoring.\n\n'
        md += 'The complete machine-readable missing-obligation and capability lists are in [coverage.json](coverage.json), with scope and individual obligations in [inventory.json](inventory.json). Passing behavior remains unverified.\n\n'
        md += '## Scoped execution capabilities\n\n'
        md += f'Registered means membership in the default {len(registered)}-entry registry. Entries below describe bounded policy-specific support; neither status establishes complete compatibility or live runtime verification.\n\n'
        for feature in report['scoped_capabilities']:
            scope = feature['implementation_scope']
            md += '- **' + feature['id'] + '** (' + feature['implementation_status'] + '; ' + ', '.join(scope['policies']) + '): ' + scope['supported'] + ' Limits: ' + ' '.join(scope['limitations']) + '\n'
        md += '\n'
        md += '## Explicit capability gaps\n\n' + '\n'.join('- ' + f['id'] for f in report['unsupported_features']) + '\n'
        (ROOT / 'test-specs/coverage.md').write_text(md)
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
