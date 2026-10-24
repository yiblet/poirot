"""Independent arithmetic and coordinate controls for optional storage diagnostics."""
import unittest

from storage_workloads import column_name, generate_storage


class StorageWorkloads(unittest.TestCase):
    """Reject malformed bounds and accidental differences between physical layouts."""

    def test_arithmetic_and_spill_expectations(self):
        cases = {c["name"]: c for c in generate_storage(4, 1, 0, False)["cases"]}
        for name, workload in cases.items():
            if name.startswith("layout_"):
                self.assertEqual(workload["expected"], [3, 5, 7, 9])
                self.assertEqual(workload["edits"][0]["expected"], [5, 5, 7, 9])
                self.assertEqual(workload["edits"][1]["expected"], [7, 5, 7, 9])
        self.assertEqual(cases["many_spill_producers"]["expected"], [4, 5, 6, 7])
        self.assertEqual(cases["many_spill_producers"]["edits"][1]["expected"], [6, 7, 8, 9])
        self.assertEqual(cases["large_spill"]["expected"], list(range(1, 17)))
        self.assertEqual(cases["large_spill"]["edits"][0]["expected"], list(range(2, 18)))
        self.assertEqual(cases["scalar_array_transition"]["expected"], [7, 1])
        self.assertEqual(cases["scalar_array_transition"]["edits"][0]["expected"], [1, 14])
        self.assertEqual(cases["scalar_array_transition"]["edits"][1]["expected"], [7, 1])

    def test_unique_bounded_coordinates_and_edits(self):
        for n in [4, 33, 1024, 5000]:
            for workload in generate_storage(n, 1, 0, False)["cases"]:
                keys = {(c.get("sheet", 0), c["row"], c["col"]) for c in workload["cells"]}
                self.assertEqual(len(keys), len(workload["cells"]))
                for sheet, row, col in keys:
                    self.assertTrue(0 <= sheet < workload.get("sheet_count", 1))
                    self.assertTrue(0 <= row < 1048576 and 0 <= col < 16384)
                for output in workload["outputs"]:
                    self.assertTrue(0 <= output.get("sheet", 0) < workload.get("sheet_count", 1))
                    self.assertTrue(0 <= output["row"] < 1048576 and 0 <= output["col"] < 16384)
                self.assertEqual(len(workload["expected"]), len(workload["outputs"]))
                for change in workload["edits"]:
                    self.assertIn((change.get("sheet", 0), change["row"], change["col"]), keys)
                    self.assertEqual(len(change["expected"]), len(workload["outputs"]))

    def test_deterministic_bounded_generation_and_references(self):
        self.assertEqual(generate_storage(33), generate_storage(33))
        self.assertEqual(len(generate_storage(4)["cases"]), 16)
        self.assertEqual(len(generate_storage(4, include_base=False)["cases"]), 9)
        self.assertEqual([column_name(i) for i in [0, 25, 26, 701, 702, 16383]],
                         ["A", "Z", "AA", "ZZ", "AAA", "XFD"])
        for args in [(3, 1, 0), (5001, 1, 0), (4, 0, 0), (4, 1, -1)]:
            with self.assertRaises(ValueError):
                generate_storage(*args)


if __name__ == "__main__":
    unittest.main()
