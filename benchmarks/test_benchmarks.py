"""Contract and failure-path checks for the benchmark generator and report protocol."""
import copy
import unittest

from run import ENGINES, markdown, summarize, validate_report
from diagnostics import generate_diagnostics
from workloads import generate


class BenchmarkContracts(unittest.TestCase):
    """Keep common workload expectations and accepted measurement sets inspectable."""

    def test_small_workload_results_come_from_arithmetic(self):
        suite = generate(4, 1, 0)
        cases = {case["name"]: case for case in suite["cases"]}
        # Sum 1..4 = 10; arithmetic rows are 4.25, 5.5, 6.75, 8.
        self.assertEqual(cases["arithmetic"]["expected"], [24.5])
        self.assertEqual(cases["fan_out"]["expected"], [10])
        self.assertEqual(cases["prefix_sums"]["expected"], [1, 3, 6, 10])
        # IF(row > 2, row*2, row+1) yields 2, 3, 6, 8.
        self.assertEqual(cases["conditional"]["expected"], [19])
        self.assertEqual(cases["exact_lookup"]["expected"], [30])
        self.assertEqual(cases["dependency_chain"]["expected"], [5])
        self.assertEqual(cases["independent_branches"]["edits"][0]["expected"], [4, 4, 6, 8])
        self.assertEqual(cases["prefix_sums"]["edits"][1]["expected"], [3, 5, 8, 12])

    def test_inputs_are_deterministic_and_parameters_are_bounded(self):
        self.assertEqual(generate(), generate())
        for args in [(3, 1, 0), (5001, 1, 0), (4, 0, 0), (4, 1, -1)]:
            with self.assertRaises(ValueError):
                generate(*args)

    def report(self):
        """Create a complete tiny protocol response independent of any adapter."""
        suite = generate(4, 2, 1)
        timings = []
        for case in suite["cases"]:
            for sample in range(suite["samples"]):
                for phase, edit in [("build_calculate_read", None), ("edit_calculate_read", 0), ("edit_calculate_read", 1)]:
                    timings.append(dict(case=case["name"], phase=phase, sample=sample, edit=edit, elapsed_ns=100))
        return suite, dict(schema_version=1, engine="hyperformula", mode="automatic", version="3.5.0", timings=timings)

    def test_rejects_missing_duplicate_unexpected_or_invalid_measurements(self):
        suite, report = self.report()
        validate_report(report, suite, "hyperformula")
        changes = [lambda r: r["timings"].pop(),
                   lambda r: r["timings"].append(r["timings"][0]),
                   lambda r: r["timings"][0].update(elapsed_ns=0),
                   lambda r: r["timings"][0].update(elapsed_ns=float("nan")),
                   lambda r: r["timings"][0].update(phase="cached_read"),
                   lambda r: r.update(version="unknown")]
        for change in changes:
            bad = copy.deepcopy(report)
            change(bad)
            with self.assertRaises(ValueError):
                validate_report(bad, suite, "hyperformula")

    def test_batch_workflows_have_distinct_protocol_identities(self):
        suite, report = self.report()
        for selection in ["poirot-evaluation", "poirot-overlay", "poirot-evaluation-batch", "poirot-overlay-batch"]:
            _, _, engine, mode, version = ENGINES[selection]
            report.update(engine=engine, mode=mode, version=version)
            validate_report(report, suite, selection)
            for other in ["poirot-evaluation", "poirot-overlay", "poirot-evaluation-batch", "poirot-overlay-batch"]:
                if other != selection:
                    with self.assertRaises(ValueError):
                        validate_report(report, suite, other)

    def test_java_engines_require_their_exact_native_identity(self):
        suite, report = self.report()
        for selection in ["apache-poi", "libreoffice"]:
            _, _, engine, mode, version = ENGINES[selection]
            report.update(engine=engine, mode=mode, version=version)
            validate_report(report, suite, selection)
            for field in ["engine", "mode", "version"]:
                bad = copy.deepcopy(report)
                bad[field] = "unverified"
                with self.assertRaises(ValueError):
                    validate_report(bad, suite, selection)

    def test_fixed_range_diagnostics_have_independent_expected_values(self):
        cases = {case["name"]: case for case in generate_diagnostics(4, 1, 0)["cases"]}
        for name in ["fixed_disjoint_ranges", "fixed_shared_top", "fixed_shared_bottom"]:
            self.assertEqual(cases[name]["expected"], [10] * 4)  # 1 + 2 + 3 + 4.
        self.assertEqual(cases["fixed_disjoint_ranges"]["edits"][0]["expected"], [11, 10, 10, 10])
        self.assertEqual(cases["fixed_shared_bottom"]["edits"][1]["expected"], [12] * 4)
        top = cases["fixed_shared_top"]["cells"]
        bottom = cases["fixed_shared_bottom"]["cells"]
        self.assertEqual([c["row"] for c in top[:4]], [0, 1, 2, 3])
        self.assertEqual([c["row"] for c in bottom[:4]], [16, 17, 18, 19])

    def test_summary_keeps_edit_ordinals_separate(self):
        _, report = self.report()
        rows = summarize({"hyperformula": report})
        self.assertEqual(len(rows), 7 * 3)
        self.assertTrue(all(row["median_ns"] == 100 and row["samples"] == 2 for row in rows))
        self.assertIn("0.0001", markdown(rows))


if __name__ == "__main__":
    unittest.main()
