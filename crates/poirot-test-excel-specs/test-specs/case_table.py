"""Read/write the one normative case table, shared with the combined JSON runner."""
import json


def table_path(root):
    local = root / 'test-specs/cases.json'
    # Maintenance checks materialize the table only in an isolated scratch package.
    if local.is_file():
        return local
    return root.parent / 'poirot-test-runner/corpus/json/cases.json'


def read_table(root):
    return json.loads(table_path(root).read_text())


def write_table(root, cases):
    table_path(root).write_text(json.dumps(cases, ensure_ascii=True, indent=2) + '\n')
