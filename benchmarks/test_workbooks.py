"""Contracts for extracting workbook formula probes and pinning original bytes."""

import hashlib
import io
import json
from pathlib import Path
from subprocess import CompletedProcess
import tempfile
import unittest
from unittest.mock import patch
from zipfile import ZipFile

from workbooks import acquire, coordinates, formula_probes, main, run_case


class WorkbookContracts(unittest.TestCase):
    """Keep workbook coordinates, cached values, and input identity explicit."""

    def test_formula_probes_follow_workbook_relationships(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "book.xlsx"
            with ZipFile(path, "w") as book:
                book.writestr("xl/workbook.xml", """
                    <workbook xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
                      <sheets>
                        <sheet name="Report" r:id="rid9"/>
                        <sheet name="Inputs" r:id="rid4"/>
                      </sheets>
                    </workbook>""")
                book.writestr("xl/_rels/workbook.xml.rels", """
                    <Relationships>
                      <Relationship Id="rid4" Type="urn:example/worksheet" Target="/xl/worksheets/data.xml"/>
                      <Relationship Id="rid9" Type="urn:example/worksheet" Target="worksheets/report.xml"/>
                    </Relationships>""")
                book.writestr("xl/worksheets/data.xml", """
                    <worksheet><sheetData><row><c r="A1"><v>3</v></c></row></sheetData></worksheet>""")
                book.writestr("xl/worksheets/report.xml", """
                    <worksheet><sheetData><row>
                      <c r="AB3"><f>Inputs!A1*2</f><v>6</v></c>
                      <c r="AC3" t="b"><f>AB3=6</f><v>1</v></c>
                      <c r="AD3"><f>AB3+1</f></c>
                      <c r="AE3" t="str"><f>IF(AC3,"","no")</f><v/></c>
                      <c r="AF3" t="str"><f>AE3&amp;"yes"</f><v>yes</v></c>
                      <c r="AG3" t="str"><f>CHAR(10)</f><v>_x000A_</v></c>
                    </row></sheetData></worksheet>""")
            self.assertEqual(formula_probes(path), [
                dict(sheet=0, sheet_name="Report", row=2, col=27, expected=6.0),
                dict(sheet=0, sheet_name="Report", row=2, col=28, expected=True),
                dict(sheet=0, sheet_name="Report", row=2, col=29, expected=None),
                dict(sheet=0, sheet_name="Report", row=2, col=30, expected=""),
                dict(sheet=0, sheet_name="Report", row=2, col=31, expected="yes"),
                dict(sheet=0, sheet_name="Report", row=2, col=32, expected=None),
            ])

    def test_inputs_with_changed_bytes_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "pinned.xlsx"
            path.write_bytes(b"original bytes")
            record = dict(id="pinned", sha256=hashlib.sha256(path.read_bytes()).hexdigest())
            self.assertEqual(acquire(record, root, False), path)
            path.write_bytes(b"different bytes")
            with self.assertRaisesRegex(ValueError, "digest mismatch"):
                acquire(record, root, False)

    def test_malformed_coordinate_is_refused(self):
        with self.assertRaisesRegex(ValueError, "invalid cell address"):
            coordinates("A0")

    def test_timing_blocks_alternate_order_and_discard_results_from_a_failing_engine(self):
        calls = []

        def run(command, **_):
            engine = command[1]
            calls.append(engine)
            if engine == "ironcalc" and calls.count(engine) == 2:
                return CompletedProcess(command, 1, "", "incorrect output")
            sample = {"import_ns": 10, "calculate_ns": 5}
            Path(command[-1]).write_text(json.dumps([sample]))
            return CompletedProcess(command, 0, "workbook importer diagnostic\n", "")

        with (patch("workbooks.acquire", return_value=Path("book.xlsx")),
              patch("workbooks.formula_probes", return_value=[{"expected": 1.0}]),
              patch("workbooks.subprocess.run", side_effect=run)):
            result = run_case({}, None, Path("workbook-speed"), 1, 0, 2, False,
                              None, None, blocks=3, reverse_first=True)

        self.assertEqual(calls, ["ironcalc", "poirot", "poirot", "ironcalc", "poirot"])
        self.assertEqual([sample["block"] for sample in result["poirot"]["samples"]], [0, 1, 2])
        self.assertEqual(result["ironcalc"], dict(error="incorrect output", exit_code=1,
                                                 failed_block=1))

    def test_invalid_timing_values_exclude_the_entire_engine_result(self):
        valid = {"import_ns": 10, "calculate_ns": 5}
        invalid = {"import_ns": 10, "calculate_ns": -5}
        outputs = iter([valid, valid, invalid, valid])

        def run(command, **_):
            Path(command[-1]).write_text(json.dumps([next(outputs)]))
            return CompletedProcess(command, 0, "", "")

        with (patch("workbooks.acquire", return_value=Path("book.xlsx")),
              patch("workbooks.formula_probes", return_value=[{"expected": 1.0}]),
              patch("workbooks.subprocess.run", side_effect=run)):
            result = run_case({}, None, Path("workbook-speed"), 1, 0, 2, False,
                              None, None, blocks=2)

        self.assertEqual(len(result["poirot"]["samples"]), 2)
        self.assertEqual(result["ironcalc"], dict(error="invalid workbook timing sample",
                                                 failed_block=1))

    def test_report_records_the_executable_digest(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "models.json"
            binary = root / "workbook-speed"
            manifest.write_text('[{"id": "sample"}]')
            binary.write_bytes(b"frozen release executable")
            output = io.StringIO()
            with (patch("sys.argv", ["workbooks.py", str(manifest), directory, str(binary)]),
                  patch("sys.stdout", output),
                  patch("workbooks.run_case", return_value={"id": "sample"})):
                main()

        self.assertEqual(json.loads(output.getvalue())["binary_sha256"],
                         hashlib.sha256(b"frozen release executable").hexdigest())

    def test_manifest_keeps_pinned_sources_and_original_notices(self):
        directory = Path(__file__).with_name("workbook-sources")
        models = json.loads((directory / "models.json").read_text())
        self.assertEqual(len(models), len({model["id"] for model in models}))
        for model in models:
            with self.subTest(workbook=model["id"]):
                self.assertRegex(model["sha256"], r"^[0-9a-f]{64}$")
                revision = model["revision"]
                if revision.startswith("10.17632/"):
                    self.assertRegex(revision, r"^10\.17632/[a-z0-9]+\.[1-9][0-9]*$")
                    dataset, version = revision.removeprefix("10.17632/").rsplit(".", 1)
                    self.assertEqual(model["source"],
                                     f"https://data.mendeley.com/datasets/{dataset}/{version}")
                    self.assertEqual(model["license_source"], model["source"])
                    self.assertRegex(model["download"],
                                     rf"^https://data\.mendeley\.com/public-files/datasets/{dataset}/files/[0-9a-f-]{{36}}/file_downloaded$")
                elif revision.startswith("10.17863/CAM."):
                    self.assertRegex(revision, r"^10\.17863/CAM\.[1-9][0-9]*$")
                    self.assertRegex(model["source"],
                                     r"^https://www\.repository\.cam\.ac\.uk/items/[0-9a-f-]{36}$")
                    self.assertEqual(model["license_source"], model["source"])
                    self.assertRegex(model["download"],
                                     r"^https://www\.repository\.cam\.ac\.uk/bitstreams/[0-9a-f-]{36}/download$")
                else:
                    self.assertRegex(revision, r"^[0-9a-f]{40}$")
                    for field in ("source", "download", "license_source"):
                        self.assertIn(revision, model[field])
                notice = (directory / model["notice"]).read_text()
                if model["license"] == "MIT":
                    self.assertIn(model["author"], notice)
                    self.assertIn("MIT License", notice)
                else:
                    self.assertEqual(model["license"], "CC-BY-4.0")
                    self.assertIn("Attribution 4.0 International", notice)
                    self.assertTrue(model["author"])
