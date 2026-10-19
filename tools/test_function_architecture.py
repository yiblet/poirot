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
                self.assertTrue(dependency in families or dependency in {'poirot-numeric', 'poirot-coercionv2', 'poirot-parsing', 'poirot-text'},
                                f'{name} imports engine dependency {dependency}')
                pending.append(dependency)

    def test_parsing_has_no_engine_or_function_dependencies(self):
        manifest = tomllib.loads((ROOT / 'crates/poirot-parsing/Cargo.toml').read_text())
        for dependency in manifest.get('dependencies', {}):
            self.assertFalse(dependency.startswith('poirot-'),
                             f'parsing syntax depends on engine/function crate {dependency}')


if __name__ == '__main__':
    unittest.main()
