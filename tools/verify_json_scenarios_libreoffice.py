#!/usr/bin/env python3
"""Corroborate JSON scenario expectations with an isolated LibreOffice process.

Requires LibreOffice and Python UNO (typically /usr/bin/python3 with python3-uno).
This optional research tool does not run in cargo test and never changes scenarios
or their expected values. It emits a source-hash-bound record of typed Calc results.

Example:
    /usr/bin/python3 tools/verify_json_scenarios_libreoffice.py \
        crates/poirot-test-runner/corpus/json/closedxml-*.json --output /tmp/calc.json
"""

import argparse
import datetime
import hashlib
import json
import re
import subprocess
import tempfile
import time
import uuid
import zipfile
from pathlib import Path
from xml.etree import ElementTree as ET
from xml.sax.saxutils import escape, quoteattr

import uno
from com.sun.star.beans import PropertyValue
from com.sun.star.connection import NoConnectException

NS = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
REL = "http://schemas.openxmlformats.org/package/2006/relationships"
OFFICE_REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
FUTURE_FUNCTIONS = {
    "ARABIC", "BASE", "CEILING.MATH", "CEILING.PRECISE", "COMBINA", "CONCAT",
    "COVARIANCE.P", "COVARIANCE.S", "DECIMAL", "DAYS", "FLOOR.MATH",
    "FLOOR.PRECISE", "IFS", "ISOWEEKNUM", "MAXIFS", "MINIFS", "NUMBERVALUE",
    "SWITCH", "TEXTJOIN", "IFNA", "XLOOKUP", "XMATCH", "RANK.EQ",
    "PERCENTILE.INC", "QUARTILE.INC", "PERCENTRANK.INC", "PERCENTRANK.EXC",
    "NORM.DIST", "NORM.INV", "NORM.S.DIST", "NORM.S.INV", "POISSON.DIST",
    "T.DIST", "T.DIST.RT", "T.DIST.2T", "FORECAST.LINEAR", "UNICHAR", "UNICODE",
}


def xml_text(text):
    # XML normalizes literal CR/CRLF; character references retain UTF-16 lengths.
    return escape(text).replace("\r", "&#13;")


def excel_formula(formula):
    parts = re.split(r'("(?:[^"]|"")*")', formula)
    for i in range(0, len(parts), 2):
        parts[i] = re.sub(
            r"\b([A-Za-z][A-Za-z0-9_.]*)\s*(?=\()",
            lambda m: ("_xlfn." if m[1].upper() in FUTURE_FUNCTIONS else "") + m[0],
            parts[i],
        )
    return "".join(parts)


def cell_xml(address, content):
    if "formula" in content:
        formula = xml_text(excel_formula(content["formula"]))
        return f'<c r="{address}"><f>{formula}</f></c>'
    kind = content["type"]
    if kind == "blank":
        return f'<c r="{address}"/>'
    value = content["value"]
    if kind == "number":
        return f'<c r="{address}" t="n"><v>{value}</v></c>'
    if kind == "boolean":
        return f'<c r="{address}" t="b"><v>{int(value)}</v></c>'
    if kind == "error":
        return f'<c r="{address}" t="e"><v>{xml_text(value)}</v></c>'
    if kind == "text":
        if value == "":
            # Calc discards literal empty strings during OOXML import. A formula
            # supplies exactly the required empty-text value to reference consumers.
            return f'<c r="{address}"><f>""</f></c>'
        return (
            f'<c r="{address}" t="inlineStr"><is><t xml:space="preserve">'
            f"{xml_text(value)}</t></is></c>"
        )
    raise ValueError(f"Unsupported fixture scalar type: {kind}")


def write_fixture(path, sheets, cells):
    """Write minimal OOXML test fixtures, with uncached formulas for Calc to run."""
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        overrides = "".join(
            f'<Override PartName="/xl/worksheets/sheet{i}.xml" '
            'ContentType="application/vnd.openxmlformats-officedocument.'
            'spreadsheetml.worksheet+xml"/>'
            for i in range(1, len(sheets) + 1)
        )
        archive.writestr(
            "[Content_Types].xml",
            '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-'
            'package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/>'
            '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.'
            'openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
            f"{overrides}</Types>",
        )
        archive.writestr(
            "_rels/.rels",
            f'<Relationships xmlns="{REL}"><Relationship Id="rId1" '
            f'Type="{OFFICE_REL}/officeDocument" Target="xl/workbook.xml"/>'
            "</Relationships>",
        )
        sheet_elements = "".join(
            f'<sheet name={quoteattr(name)} sheetId="{i}" r:id="rId{i}"/>'
            for i, name in enumerate(sheets, 1)
        )
        archive.writestr(
            "xl/workbook.xml",
            f'<workbook xmlns="{NS}" xmlns:r="{OFFICE_REL}"><sheets>'
            f'{sheet_elements}</sheets><calcPr fullCalcOnLoad="1"/></workbook>',
        )
        archive.writestr(
            "xl/_rels/workbook.xml.rels",
            f'<Relationships xmlns="{REL}">'
            + "".join(
                f'<Relationship Id="rId{i}" Type="{OFFICE_REL}/worksheet" '
                f'Target="worksheets/sheet{i}.xml"/>'
                for i in range(1, len(sheets) + 1)
            )
            + "</Relationships>",
        )
        for i, name in enumerate(sheets, 1):
            rows = {}
            for qualified, content in cells.items():
                sheet, address = qualified.rsplit("!", 1)
                if sheet.strip("'") != name:
                    continue
                row = int(re.search(r"\d+$", address)[0])
                column = re.match(r"[A-Z]+", address)[0]
                # Length then lexical order is worksheet column order: Z before AA.
                rows.setdefault(row, []).append(
                    ((len(column), column), cell_xml(address, content))
                )
            data = "".join(
                f'<row r="{row}">'
                + "".join(cell for _, cell in sorted(rows[row]))
                + "</row>"
                for row in sorted(rows)
            )
            archive.writestr(
                f"xl/worksheets/sheet{i}.xml",
                f'<worksheet xmlns="{NS}"><sheetData>{data}</sheetData></worksheet>',
            )


def read_cached(path, sheet_index, address):
    """Read the type serialized by Calc, avoiding formatted-string inference."""
    ns = {"s": NS}
    with zipfile.ZipFile(path) as archive:
        root = ET.fromstring(archive.read(f"xl/worksheets/sheet{sheet_index + 1}.xml"))
        cell = root.find(f'.//s:c[@r="{address}"]', ns)
        if cell is None:
            return {"type": "blank"}
        kind = cell.get("t", "n")
        value_element = cell.find("s:v", ns)
        value = "" if value_element is None else (value_element.text or "")
        if kind == "b":
            return {"type": "boolean", "value": value == "1"}
        if kind == "e":
            return {"type": "error", "value": value}
        if kind == "s":
            strings = ET.fromstring(archive.read("xl/sharedStrings.xml"))
            item = strings.findall("s:si", ns)[int(value)]
            return {"type": "text", "value": "".join(t.text or "" for t in item.findall(".//s:t", ns))}
        if kind in ("str", "inlineStr"):
            if kind == "inlineStr":
                value = "".join(t.text or "" for t in cell.findall(".//s:t", ns))
            return {"type": "text", "value": value}
        if kind != "n":
            raise ValueError(f"Unsupported cached OOXML type: {kind}")
        return {"type": "number", "value": float(value)} if value else {"type": "blank"}


def agrees(actual, action):
    expected = action["expected"]
    if actual["type"] != expected["type"]:
        return False
    if expected["type"] == "number":
        return abs(actual["value"] - expected["value"]) <= action.get("tolerance", 0)
    return actual == expected


def prop(name, value):
    result = PropertyValue()
    result.Name = name
    result.Value = value
    return result


def connect(pipe, process):
    context = uno.getComponentContext()
    resolver = context.ServiceManager.createInstanceWithContext(
        "com.sun.star.bridge.UnoUrlResolver", context
    )
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError("Isolated LibreOffice process exited during startup")
        try:
            remote = resolver.resolve(f"uno:pipe,name={pipe};urp;StarOffice.ComponentContext")
            return remote.ServiceManager.createInstanceWithContext(
                "com.sun.star.frame.Desktop", remote
            )
        except NoConnectException:
            time.sleep(0.1)
    raise RuntimeError("Timed out connecting to isolated LibreOffice")


def verify(suite_path, desktop, directory):
    suite = json.loads(suite_path.read_text())
    sheets = suite["workbook"]["sheets"]
    source, output = directory / "input.xlsx", directory / "output.xlsx"
    for scenario in suite["scenarios"]:
        cells = {c["cell"]: c["content"] for c in suite["workbook"].get("cells", [])}
        checks = []
        for i, action in enumerate(scenario["actions"]):
            if action["action"] == "set":
                cells[action["cell"]] = action["content"]
                continue
            if action["action"] != "check" or any(
                field in action for field in ("number_format", "display")
            ):
                raise ValueError("This verifier handles scalar checks, not formatting")
            # ISFORMULA/FORMULATEXT would observe the empty-text materialization.
            if any(re.search(r"\b(?:ISFORMULA|FORMULATEXT|CELL)\s*\(", c.get("formula", ""), re.I) for c in cells.values()):
                raise ValueError("Formula-origin introspection needs a different oracle fixture")
            write_fixture(source, sheets, cells)
            document = desktop.loadComponentFromURL(
                source.as_uri(), "_blank", 0, (prop("Hidden", True),)
            )
            if document is None:
                raise RuntimeError(f"Failed to open fixture for {scenario['name']}")
            try:
                document.calculateAll()
                document.storeAsURL(
                    output.as_uri(),
                    (prop("FilterName", "Calc MS Excel 2007 XML"), prop("Overwrite", True)),
                )
            finally:
                document.close(True)
            sheet, address = action["cell"].rsplit("!", 1)
            actual = read_cached(output, sheets.index(sheet.strip("'")), address)
            checks.append({"action": i, "actual": actual, "agrees": agrees(actual, action)})
        yield {
            "suite": suite_path.name,
            "name": scenario["name"],
            "checks": checks,
            "agrees": all(check["agrees"] for check in checks),
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("suites", nargs="+", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    report = {
        "engine": subprocess.check_output(["libreoffice", "--version"], text=True).strip(),
        "verified_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "method": "Fresh OOXML snapshot for every check; hidden import, calculateAll, export; typed cached results. Empty input text is materialized as =\"\". No Poirot results are used.",
        "scope": "Scalar values only. Rebuilding snapshots verifies values after authored edits; cargo tests verify persistent invalidation and normal/overlay change lists.",
        "suite_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in args.suites},
        "results": [],
    }
    with tempfile.TemporaryDirectory(prefix="poirot-libreoffice-") as temporary:
        directory = Path(temporary)
        profile = directory / "profile"
        (profile / "user").mkdir(parents=True)
        (profile / "user/registrymodifications.xcu").write_text(
            '<oor:items xmlns:oor="http://openoffice.org/2001/registry">'
            '<item oor:path="/org.openoffice.Setup/L10N">'
            '<prop oor:name="ooLocale" oor:op="fuse"><value>en-US</value></prop>'
            '</item></oor:items>'
        )
        pipe = "poirot_" + uuid.uuid4().hex
        process = subprocess.Popen(
            ["libreoffice", f"-env:UserInstallation={profile.as_uri()}", "--headless",
             "--norestore", "--nodefault", "--nofirststartwizard",
             f"--accept=pipe,name={pipe};urp;StarOffice.ComponentContext"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        try:
            desktop = connect(pipe, process)
            for suite in args.suites:
                for result in verify(suite, desktop, directory):
                    report["results"].append(result)
                    if len(report["results"]) % 25 == 0:
                        print(f"Verified {len(report['results'])} scenarios", flush=True)
        finally:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
    results = report["results"]
    report["summary"] = {
        "scenarios": len(results),
        "checks": sum(len(r["checks"]) for r in results),
        "corroborated_scenarios": sum(r["agrees"] for r in results),
        "corroborated_checks": sum(c["agrees"] for r in results for c in r["checks"]),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=True, indent=2) + "\n")
    print(json.dumps(report["summary"]))
    return 0 if results and all(r["agrees"] for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
