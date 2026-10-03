"""Regenerate or check package-local specs, evidence indexes and coverage reports."""
import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUTS = ['cases.json', 'sources.json', 'coverage.json', 'coverage.md']


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
        # Every required input is copied from this package. No repository root,
        # external fixture directory or remembered /tmp artifact is an input.
        shutil.copytree(ROOT / 'test-specs', scratch / 'test-specs', ignore=shutil.ignore_patterns('__pycache__'))
        shutil.copytree(ROOT / 'tests', scratch / 'tests')
        shutil.copytree(ROOT / 'tools', scratch / 'tools', ignore=shutil.ignore_patterns('__pycache__'))
        rebuild(scratch)
        for name in OUTPUTS:
            generated, checked_in = scratch / 'test-specs' / name, ROOT / 'test-specs' / name
            if name.endswith('.json'):
                assert json.loads(generated.read_text()) == json.loads(checked_in.read_text()), f'Stale generated file: {name}'
            else:
                assert generated.read_text() == checked_in.read_text(), f'Stale generated file: {name}'
    print('Package-local regeneration and coverage checks passed.')


if __name__ == '__main__':
    main()
