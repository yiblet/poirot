"""Opt-in schema2 literal/source/sheet contracts for the isolated external adapters."""
import copy
import json
import os
import subprocess
import unittest
import uuid

from run import ENGINES, validate_report
from workloads import generate


@unittest.skipUnless(os.environ.get("POIROT_BENCH_SCHEMA2_TESTS") == "1",
                     "set POIROT_BENCH_SCHEMA2_TESTS=1 after rebuilding external images")
class Schema2Adapters(unittest.TestCase):
    """Exercise real public setters and reject unversioned or ambiguous source inputs."""

    def invoke(self, engine, suite):
        """Keep resource cleanup bounded even after a failed adapter process."""
        image, mode, *_ = ENGINES[engine]
        container = "poirot-schema2-test-" + uuid.uuid4().hex
        tag = f"poirot-bench-{image}:{os.environ.get('POIROT_BENCH_IMAGE_SUFFIX', 'local')}"
        command = ["docker", "run", "--rm", "--name", container, "-i", "--network", "none", "--read-only",
                   "--tmpfs", "/tmp:rw,size=64m", "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
                   "--pids-limit", "256", "--cpus", "1", "--memory", "2g", "--memory-swap", "2g", tag, *mode]
        try:
            return subprocess.run(command, input=json.dumps(suite), text=True, capture_output=True, timeout=60)
        finally:
            subprocess.run(["docker", "rm", "--force", container], capture_output=True, timeout=30)

    def test_cross_sheet_literal_and_formula_replacement_workflows(self):
        suite = dict(schema_version=2, samples=1, warmup=0, unchanged_read=True, cases=[dict(
            name="source_replacements", sheet_count=2,
            cells=[dict(sheet=1, row=0, col=0, input={"text": "=A1"}),
                   dict(row=0, col=1, input="=LEN(Sheet2!A1)*2")],
            outputs=[dict(row=0, col=1)], expected=[6],
            edits=[dict(row=0, col=1, value="=LEN(Sheet2!A1)*3", expected=[9]),
                   dict(sheet=1, row=0, col=0, value={"text": "plain"}, expected=[15]),
                   dict(row=0, col=1, value=42, expected=[42])])])
        for engine in ["hyperformula", "hyperformula-array", "apache-poi", "libreoffice"]:
            with self.subTest(engine=engine):
                result = self.invoke(engine, suite)
                self.assertEqual(result.returncode, 0, result.stderr)
                validate_report(json.loads(result.stdout), suite, engine)

    def test_schema1_features_and_ambiguous_edits_fail_before_publication(self):
        base = generate(4, 1, 0)
        base["cases"] = base["cases"][:1]
        fixtures = []
        bad = copy.deepcopy(base)
        bad["unchanged_read"] = True
        fixtures.append(bad)
        bad = copy.deepcopy(base)
        bad["cases"][0]["sheet_count"] = 2
        fixtures.append(bad)
        bad = copy.deepcopy(base)
        bad["cases"][0]["edits"][0]["value"] = "=2"
        fixtures.append(bad)
        bad = copy.deepcopy(base)
        bad["schema_version"] = 2
        bad["cases"][0]["edits"][0]["value"] = {"text": "x", "formula": "=2"}
        fixtures.append(bad)
        for engine in ["hyperformula", "apache-poi", "libreoffice"]:
            for index, suite in enumerate(fixtures):
                with self.subTest(engine=engine, fixture=index):
                    result = self.invoke(engine, suite)
                    self.assertNotEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout.strip(), "")


if __name__ == "__main__":
    unittest.main()
