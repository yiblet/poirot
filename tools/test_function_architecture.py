"""Keep published boundaries and pure algorithm modules independent of workbook state."""
from pathlib import Path
import tomllib
import unittest

from test_coercion_architecture import production_code

ROOT = Path(__file__).resolve().parents[1]


class FunctionArchitectureTests(unittest.TestCase):
    def test_production_packages_have_purposeful_dependency_boundaries(self):
        manifests = {"poirot": tomllib.loads((ROOT / "Cargo.toml").read_text())}
        for path in (ROOT / "crates").glob("*/Cargo.toml"):
            manifest = tomllib.loads(path.read_text())
            manifests[manifest["package"]["name"]] = manifest
        published = {name for name, manifest in manifests.items()
                     if manifest["package"].get("publish") is not False}
        self.assertEqual(published, {"poirot", "poirot-parsing", "poirot-functions", "poirot-io"})
        allowed = {
            "poirot-parsing": set(),
            "poirot-functions": {"poirot-parsing"},
            "poirot-io": {"poirot-parsing", "poirot-functions"},
            "poirot": {"poirot-parsing", "poirot-functions", "poirot-io"},
        }
        for name, dependencies in allowed.items():
            sections = [manifests[name], *manifests[name].get("target", {}).values()]
            actual = set()
            for section in sections:
                for kind in ("dependencies", "build-dependencies"):
                    for alias, specification in section.get(kind, {}).items():
                        dependency = specification.get("package", alias) if isinstance(specification, dict) else alias
                        if dependency.startswith("poirot"):
                            actual.add(dependency)
            self.assertLessEqual(actual, dependencies, f"{name} imports state outside its responsibility")

    def test_algorithm_and_codec_modules_do_not_import_workbook_implementations(self):
        for name in ("poirot-functions", "poirot-io", "poirot-parsing"):
            source = ROOT / "crates" / name / "src"
            for path in source.rglob("*.rs"):
                if "test" in path.name:
                    continue
                code = production_code(path.read_text())
                self.assertNotRegex(code, r"\bpoirot\s*::|\bcrate\s*::\s*(?:model|eval|workflow)\b", str(path))



if __name__ == "__main__":
    unittest.main()
