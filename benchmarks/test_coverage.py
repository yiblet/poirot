"""Independent expected-value and failure-contract checks for expanded speed coverage."""
import copy
import unittest
import json
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
from unittest import mock

from coverage import generate_coverage, neighbors
from expanded import CAPABILITY_GAPS, comparison_rows, execute, main, outcome_kind, scaling_rows, validate_suite
from run import validate_report
from workloads import generate


class CoverageContracts(unittest.TestCase):
    """Keep old gates immutable and ensure new cases cannot pass stale or wrong engines."""

    def setUp(self):
        self.suite = generate_coverage(4, 1, 0)
        self.cases = {case["name"]: case for case in self.suite["cases"]}

    def test_original_seven_and_all_neighbors_are_retained(self):
        self.assertEqual(self.suite["cases"][:7], generate(4, 1, 0)["cases"])
        self.assertEqual(self.suite["cases"][:18], neighbors(4, 1, 0)["cases"])
        self.assertEqual(len(self.cases), len(self.suite["cases"]))
        self.assertEqual(self.suite, generate_coverage(4, 1, 0))
        validate_suite(self.suite)

    def test_small_expected_values_are_independent(self):
        # Diamonds: two branches (x+i) + (2*x+i), at x=1.
        self.assertEqual(self.cases["diamond_many"]["expected"], [3, 5, 7, 9])
        self.assertEqual(self.cases["diamond_single"]["expected"], [24])
        # Four sliding two-member sums: 1+2, 2+3, 3+4, 4+5.
        self.assertEqual(self.cases["sliding_windows"]["expected"], [3, 5, 7, 9])
        self.assertEqual(self.cases["sliding_rectangles"]["expected"], [7, 9, 11, 13])
        # SUMIF selecting values 2*r for r>=2: 4+6+8=18.
        self.assertEqual(self.cases["criteria_repeated"]["expected"], [18] * 4)
        self.assertEqual(self.cases["criteria_changing"]["expected"], [20, 18, 14, 8])
        self.assertEqual(self.cases["criteria_multiple"]["expected"], [12, 12, 8, 8])
        self.assertEqual(self.cases["lookup_approximate"]["expected"], [3, 6, 9, 12])
        self.assertEqual(self.cases["lookup_duplicates"]["expected"], [3] * 4)
        self.assertEqual(self.cases["lookup_missing"]["expected"], [-1] * 4)
        self.assertEqual(self.cases["lookup_missing"]["capabilities"], ["exact_lookup"])
        self.assertEqual(self.cases["lookup_literal_text"]["expected"], [12] * 4)
        self.assertEqual(self.cases["text_transform_search"]["expected"], [18] * 4)
        self.assertEqual(self.cases["business_days"]["expected"], [1, 2, 3, 4])
        self.assertEqual(self.cases["statistics_var_p"]["expected"], [1.25] * 4)
        self.assertEqual(self.cases["matrix_multiply"]["expected"], [14] * 4)
        self.assertEqual(self.cases["errors_blanks"]["expected"], [0, 0, 6, 0])
        self.assertEqual(self.cases["errors_blanks"]["edits"][1]["expected"], [4, 6, 6, 0])

    def test_spill_growth_and_shrink_observe_changed_extent(self):
        dynamic = self.cases["spill_dynamic"]
        self.assertEqual(dynamic["expected"], [1, 2, 3, 4, 0])
        self.assertEqual(dynamic["edits"][0]["expected"], [1, 2, 3, 4, 11])
        self.assertEqual(dynamic["edits"][1]["expected"], [1, 2, 3, 4, 0])
        competitor = self.cases["spill_competing_bounds"]
        self.assertIn("SEQUENCE(IF($G$1=0,1,2),2)", competitor["cells"][1]["input"])
        self.assertEqual(competitor["outputs"][0], dict(row=2, col=2))

    def test_schema1_rejects_new_features_and_invalid_edits(self):
        for changes in [dict(unchanged_read=True), dict(schema_version=7)]:
            suite = generate(4, 1, 0)
            suite.update(changes)
            with self.assertRaises(ValueError):
                validate_suite(suite)
        for field, value in [("value", "=A1*3"), ("value", {"text": "literal"}),
                             ("value", {"text": "literal", "formula": "=1"}),
                             ("sheet", 1), ("row", -1), ("value", True), ("value", "not a formula")]:
            suite = generate(4, 1, 0)
            suite["cases"][0]["edits"][0][field] = value
            with self.assertRaises(ValueError):
                validate_suite(suite)
        suite = generate(4, 1, 0)
        suite["cases"][0]["sheet_count"] = 2
        with self.assertRaises(ValueError):
            validate_suite(suite)

    def test_unknown_failures_are_not_capability_exclusions(self):
        for error in ["wrong result", "out of memory", "unsupported schema2", "missing unchanged_read"]:
            self.assertEqual(outcome_kind("ironcalc", "dynamic_arrays", error, 1)[0], "mismatch")

    def test_known_capabilities_require_exact_failure_evidence(self):
        for (engine, feature), evidence in CAPABILITY_GAPS.items():
            diagnostic = evidence["markers"][0]
            self.assertEqual(outcome_kind(engine, feature, diagnostic, 1)[0], "unsupported")
            self.assertEqual(outcome_kind(engine, feature, diagnostic, -1)[0], "mismatch")
            self.assertEqual(outcome_kind(engine, feature, "wrong numeric result", 1)[0], "mismatch")

    def test_comparison_and_growth_keep_workflows_and_edit_ordinals_separate(self):
        rows = [dict(engine=engine, size=size, case="branches", phase="edit_calculate_read", edit=step,
                     median_ns=time * size * (step + 1))
                for engine, time in [("ironcalc", 10), ("poirot-evaluation", 20)]
                for size in [4, 16] for step in [0, 1]]
        self.assertTrue(all(row["descriptive_ratio"] == 2 for row in comparison_rows(rows)))
        self.assertTrue(all(row["time_growth"] == 4 and row["descriptive_exponent"] == 1 for row in scaling_rows(rows)))
        self.assertEqual(len(scaling_rows(rows)), 4)

    def test_timeout_preserves_partial_output_command_and_resource_identity(self):
        suite = generate(4, 1, 0)
        suite["cases"] = suite["cases"][:1]
        args = SimpleNamespace(cpus=1, memory="2g", cpuset_cpus="0", rust_bin_dir=None, timeout=1)
        timeout = subprocess.TimeoutExpired(["docker"], 1, output=b'{"partial":', stderr=b"partial diagnostic")
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            with mock.patch("expanded.subprocess.run", side_effect=[timeout, SimpleNamespace(returncode=0)]):
                report, error, code, command = execute("ironcalc", suite, args,
                    {"ironcalc": {"Id": "sha256:frozen"}}, output, "timeout")
            self.assertIsNone(report)
            self.assertEqual(code, -1)
            self.assertIn("partial diagnostic", error)
            self.assertEqual((output / "timeout.stdout.json").read_text(), '{"partial":')
            invocation = json.loads((output / "invocations.json").read_text())[0]
            self.assertEqual(invocation["command"], command)
            self.assertEqual(invocation["returncode"], -1)
            self.assertGreaterEqual(invocation["ended_unix_ns"], invocation["started_unix_ns"])

    def test_duplicate_sizes_fail_before_creating_result_directories(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "results"
            with mock.patch("sys.stderr"), self.assertRaises(SystemExit):
                main(["--sizes", "16", "16", "--output", str(output)])
            self.assertFalse(output.exists())

    def test_strict_report_requires_schema2_and_the_exact_warm_phase(self):
        suite = dict(self.suite, cases=[self.cases["diamond_single"]])
        timings = [dict(case="diamond_single", phase=phase, sample=0, edit=step, elapsed_ns=100)
                   for phase, step in [("build_calculate_read", None), ("unchanged_read", None),
                                       ("edit_calculate_read", 0), ("edit_calculate_read", 1)]]
        report = dict(schema_version=2, engine="poirot", version="0.1.0", mode="evaluation", timings=timings)
        validate_report(report, suite, "poirot-evaluation")
        for malformed in [None, [], "not an object"]:
            with self.assertRaises(ValueError):
                validate_report(malformed, suite, "poirot-evaluation")
        for mutate in [lambda r: r.update(schema_version=1), lambda r: r["timings"].pop(1),
                       lambda r: r["timings"][1].update(phase="warm_evaluate_read")]:
            bad = copy.deepcopy(report)
            mutate(bad)
            with self.assertRaises(ValueError):
                validate_report(bad, suite, "poirot-evaluation")
        report.update(mode="workbook-cell")
        report["timings"][1]["phase"] = "warm_evaluate_read"
        validate_report(report, suite, "poirot-workbook-cell")


if __name__ == "__main__":
    unittest.main()
