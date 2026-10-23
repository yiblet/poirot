"""Controls for paired statistics; benchmark correctness lives in Rust harness tests."""
import unittest
from compare import adjust, paired_summary


class ComparisonTests(unittest.TestCase):
    def test_equal_pairs_are_inconclusive(self):
        report = paired_summary([(1.0, 1.0)] * 10)
        self.assertEqual(report["p"], 1.0)
        self.assertEqual(report["individual_paired_97pct_ci"], [0.0, 0.0])

    def test_known_nine_degree_t_critical_value(self):
        # A centered sequence has known variance; shift its mean to t(.985,9).
        import math
        import statistics
        centered = [float(value) - 4.5 for value in range(10)]
        shift = 2.5738039775468513 * statistics.stdev(centered) / math.sqrt(10)
        report = paired_summary([(0.0, value + shift) for value in centered])
        self.assertAlmostEqual(report["p"], 0.03, places=10)
        self.assertAlmostEqual(report["individual_paired_97pct_ci"][0], 0.0)

    def test_holm_accounts_for_multiple_comparisons(self):
        rows = [dict(p=.001, difference=-1), dict(p=.02, difference=1),
                dict(p=.04, difference=1)]
        adjust(rows)
        self.assertEqual([row["finding"] for row in rows], ["faster", "inconclusive", "inconclusive"])


if __name__ == "__main__":
    unittest.main()
