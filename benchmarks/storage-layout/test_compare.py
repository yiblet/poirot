"""Failure controls for storage comparison reports."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("storage_compare", Path(__file__).with_name("compare.py"))
comparison = importlib.util.module_from_spec(spec)
spec.loader.exec_module(comparison)


class ReportValidation(unittest.TestCase):
    def test_accepts_positive_timings_and_known_coordinate_counts(self):
        workloads = {"dense": {"cells": 8, "point_reads_ns": 100, "retained_bytes": 0}}
        self.assertEqual(comparison.validate_report({"workloads": workloads}), workloads)

    def test_rejects_nonpositive_and_malformed_timings(self):
        for value in (0, -1, True, None, "10", float("nan"), float("inf")):
            with self.subTest(value=value), self.assertRaises(ValueError):
                comparison.validate_report({"workloads": {"dense": {"cells": 8, "point_reads_ns": value}}})

    def test_rejects_missing_workloads_counts_and_timings(self):
        for report in ({}, None, {"workloads": {}}, {"workloads": {"dense": {"cells": 0, "a_ns": 1}}},
                       {"workloads": {"dense": {"cells": True, "a_ns": 1}}},
                       {"workloads": {"dense": {"cells": 8}}}):
            with self.subTest(report=report), self.assertRaises(ValueError):
                comparison.validate_report(report)


if __name__ == "__main__":
    unittest.main()
