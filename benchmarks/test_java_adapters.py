"""Opt-in Docker regression checks for Java adapters' result and publication gates."""
import copy
import json
import os
import subprocess
import unittest
import uuid

from run import validate_report
from workloads import generate


@unittest.skipUnless(os.environ.get("POIROT_BENCH_DOCKER_TESTS") == "1",
                     "set POIROT_BENCH_DOCKER_TESTS=1 after building the Java images")
class JavaAdapterFailures(unittest.TestCase):
    """Reject false numeric successes and never publish a partial timing report."""

    def test_incorrect_initial_edits_and_nonnumeric_outputs_fail(self):
        base = generate(4, 1, 0)
        base["cases"] = [base["cases"][0]]
        checks = {}
        bad = copy.deepcopy(base)
        bad["cases"][0]["expected"][0] += 1
        checks["wrong-initial"] = bad
        bad = copy.deepcopy(base)
        bad["cases"][0]["edits"][1]["expected"][0] += 1
        checks["wrong-second-edit"] = bad
        for name, formula in [("formula-text", '="text"'), ("formula-error", "=1/0")]:
            bad = copy.deepcopy(base)
            bad["cases"][0].update(cells=[dict(row=0, col=0, input=formula)],
                outputs=[dict(row=0, col=0)], expected=[0], edits=[])
            checks[name] = bad
        bad = copy.deepcopy(base)
        bad["cases"][0].update(outputs=[dict(row=20, col=20)], expected=[0], edits=[])
        checks["blank-zero"] = bad
        for engine in ["apache-poi", "libreoffice"]:
            for name, suite in checks.items():
                with self.subTest(engine=engine, case=name):
                    result = self.run_adapter(engine, suite)
                    self.assertNotEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout.strip(), "", result.stdout)

    def test_new_sparse_source_invalidates_a_cached_formula(self):
        """A new numeric cell must participate in native dependency invalidation."""
        suite = dict(schema_version=1, samples=1, warmup=0, cases=[dict(
            name="new_sparse_source",
            cells=[dict(row=0, col=0, input=1), dict(row=0, col=2, input="=A1+B2")],
            outputs=[dict(row=0, col=2)], expected=[1],
            edits=[dict(row=1, col=1, value=2, expected=[3])])])
        for engine in ["apache-poi", "libreoffice"]:
            with self.subTest(engine=engine):
                result = self.run_adapter(engine, suite)
                self.assertEqual(result.returncode, 0, result.stderr)
                validate_report(json.loads(result.stdout), suite, engine)

    def run_adapter(self, engine, suite):
        """Run one isolated fixture and remove the container even after a timeout."""
        container = "poirot-adapter-test-" + uuid.uuid4().hex
        command = ["docker", "run", "--rm", "--name", container, "-i", "--network", "none", "--read-only",
            "--tmpfs", "/tmp:rw,size=64m", "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
            "--pids-limit", "256", "--cpus", "1", "--memory", "2g", "--memory-swap", "2g",
            f"poirot-bench-{engine}:{os.environ.get('POIROT_BENCH_IMAGE_SUFFIX', 'local')}"]
        try:
            return subprocess.run(command, input=json.dumps(suite), text=True,
                capture_output=True, timeout=60)
        finally:
            subprocess.run(["docker", "rm", "--force", container], capture_output=True, timeout=30)



if __name__ == "__main__":
    unittest.main()
