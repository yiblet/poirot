"""Regenerate or check package-local specs, evidence indexes and coverage reports."""
import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'test-specs'))
from case_table import read_table

OUTPUTS = ['sources.json', 'coverage.json']


def rebuild(root):
    subprocess.run([sys.executable, str(root / 'test-specs/generate.py')], cwd=root, check=True)
    subprocess.run([sys.executable, str(root / 'tools/derive_besselj_oracle.py')], cwd=root, check=True)
    subprocess.run([sys.executable, str(root / 'tools/validate_specs.py'), '--write-report'], cwd=root, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='Rebuild in temporary scratch space and compare; leave package files unchanged.')
    args = parser.parse_args()
    if not args.check:
        rebuild(ROOT)
        return
    with tempfile.TemporaryDirectory(prefix='poirot-test-excel-specs-check-') as directory:
        scratch = Path(directory)
        # The shared normative table is materialized only in scratch; authored
        # inputs and generated outputs are otherwise copied from this package.
        shutil.copytree(ROOT / 'test-specs', scratch / 'test-specs', ignore=shutil.ignore_patterns('__pycache__'))
        shutil.copytree(ROOT / 'tests', scratch / 'tests')
        (scratch / 'test-specs/cases.json').write_text(json.dumps(read_table(ROOT), ensure_ascii=True, indent=2) + '\n')
        shutil.copytree(ROOT / 'tools', scratch / 'tools', ignore=shutil.ignore_patterns('__pycache__'))
        rebuild(scratch)
        assert json.loads((scratch / 'test-specs/cases.json').read_text()) == read_table(ROOT), 'Stale case table'
        for name in OUTPUTS:
            generated, checked_in = scratch / 'test-specs' / name, ROOT / 'test-specs' / name
            if name.endswith('.json'):
                assert json.loads(generated.read_text()) == json.loads(checked_in.read_text()), f'Stale generated file: {name}'
            else:
                assert generated.read_text() == checked_in.read_text(), f'Stale generated file: {name}'
    print('Package-local regeneration and coverage checks passed.')


if __name__ == '__main__':
    main()
