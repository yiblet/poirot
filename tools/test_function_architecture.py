"""Keep function algorithms independent of workbook and evaluator implementations."""
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]


class FunctionArchitectureTests(unittest.TestCase):
    def test_function_dependencies_remain_workbook_independent(self):
        manifests = {}
        for path in (ROOT / 'crates').glob('*/Cargo.toml'):
            manifest = tomllib.loads(path.read_text())
            manifests[manifest['package']['name']] = manifest
        families = {name for name in manifests if name.startswith('poirot-functions-')}
        visited = set()
        pending = list(families)
        while pending:
            name = pending.pop()
            if name in visited:
                continue
            visited.add(name)
            for dependency in manifests[name].get('dependencies', {}):
                if dependency not in manifests:
                    continue
                self.assertTrue(dependency in families or dependency == 'poirot-numeric',
                                f'{name} imports engine dependency {dependency}')
                pending.append(dependency)


if __name__ == '__main__':
    unittest.main()
