"""Run native XLSX comparisons using publisher-saved formula results."""

import argparse
import hashlib
import json
import math
from pathlib import Path
import posixpath
import re
import subprocess
from tempfile import TemporaryDirectory
import xml.etree.ElementTree as ET
from urllib.request import urlopen
from zipfile import BadZipFile, ZipFile

CELL = re.compile(r"([A-Z]+)([1-9][0-9]*)")
ESCAPED_TEXT = re.compile(r"_x[0-9a-fA-F]{4}_")
RELATIONSHIPS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"


def coordinates(address):
    """Convert an XLSX cell reference to zero-based row and column."""
    match = CELL.fullmatch(address)
    if match is None:
        raise ValueError(f"invalid cell address {address!r}")
    letters, row = match.groups()
    col = 0
    for letter in letters:
        col = col * 26 + ord(letter) - ord("A") + 1
    return {"row": int(row) - 1, "col": col - 1}


def formula_probes(path):
    """Enumerate XLSX formulas in sheet order and read independently comparable cached values."""
    with ZipFile(path) as book:
        metadata = ET.fromstring(book.read("xl/workbook.xml"))
        relationships = ET.fromstring(book.read("xl/_rels/workbook.xml.rels"))
        targets = {relation.get("Id"): relation.get("Target")
                   for relation in relationships
                   if relation.get("Type", "").endswith("/worksheet")
                   and relation.get("TargetMode") != "External"}
        probes = []
        for index, sheet in enumerate(metadata.findall("./{*}sheets/{*}sheet")):
            target = targets.get(sheet.get(f"{{{RELATIONSHIPS}}}id"))
            if target is None:
                continue
            path = posixpath.normpath(posixpath.join("xl", target)).lstrip("/")
            worksheet = ET.fromstring(book.read(path))
            for cell in worksheet.iterfind("./{*}sheetData/{*}row/{*}c"):
                if cell.find("{*}f") is None:
                    continue
                saved = cell.findtext("{*}v")
                kind = cell.get("t", "n")
                value = None
                if kind == "n" and saved:
                    value = float(saved)
                    if not math.isfinite(value):
                        raise ValueError("nonfinite saved formula result")
                elif kind == "b" and saved in ("0", "1"):
                    value = saved == "1"
                elif kind == "str" and saved is not None and not ESCAPED_TEXT.search(saved):
                    value = saved
                probes.append(dict(coordinates(cell.get("r", "")), sheet=index,
                                   sheet_name=sheet.get("name"), expected=value))
    return probes


def acquire(record, directory, fetch):
    """Resolve the pinned original XLSX from the local workbook cache."""
    if not re.fullmatch(r"[a-z0-9][a-z0-9.-]*", record["id"]):
        raise ValueError("invalid workbook identifier")
    path = directory / f"{record['id']}.xlsx"
    if not path.exists() and fetch:
        with urlopen(record["download"], timeout=30) as response:
            data = response.read()
    else:
        data = path.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if digest != record["sha256"]:
        raise ValueError(f"workbook digest mismatch for {path.name}")
    if not path.exists() and fetch:
        path.write_bytes(data)
    return path


def run_case(record, directory, binary, samples, warmup, timeout, fetch,
             oracle_directory, oracle_engine, blocks=1, reverse_first=False):
    """Keep validation failures visible on either side of the comparison."""
    result = dict(record)
    try:
        path = acquire(record, directory, fetch)
        probes = formula_probes(path)
        result["oracle"] = "publisher-saved formula values"
        if not any(probe["expected"] is not None for probe in probes) and oracle_directory:
            oracle = oracle_directory / path.name
            positions = {(cell["sheet_name"], cell["row"], cell["col"]): cell["expected"]
                         for cell in formula_probes(oracle)}
            for probe in probes:
                key = (probe["sheet_name"], probe["row"], probe["col"])
                probe["expected"] = positions.get(key)
            result["oracle"] = {
                "kind": "separately recalculated XLSX",
                "engine": oracle_engine,
                "file": str(oracle),
                "sha256": hashlib.sha256(oracle.read_bytes()).hexdigest(),
            }
        result["formulas"] = len(probes)
        result["numeric_checks"] = sum(isinstance(probe["expected"], float) for probe in probes)
        result["boolean_checks"] = sum(isinstance(probe["expected"], bool) for probe in probes)
        result["text_checks"] = sum(isinstance(probe["expected"], str) for probe in probes)
        if not any(probe["expected"] is not None for probe in probes):
            raise ValueError("workbook oracle has no supported cached formula results")
    except (OSError, KeyError, ValueError, ET.ParseError, BadZipFile) as error:
        return dict(result, error=str(error))
    request = json.dumps(dict(probes=probes, samples=samples, warmup=warmup), allow_nan=False)
    engines = ("poirot", "ironcalc")
    for engine in engines:
        result[engine] = dict(samples=[])
    for block in range(blocks):
        order = engines[::-1] if (block + reverse_first) % 2 else engines
        for engine in order:
            if "error" in result[engine]:
                continue
            try:
                with TemporaryDirectory(prefix="poirot-xlsx-timing-") as temporary:
                    timing_path = Path(temporary) / "timings.json"
                    run = subprocess.run([binary, engine, path, timing_path], input=request,
                                         capture_output=True, text=True, timeout=timeout, check=False)
                    if run.returncode != 0:
                        result[engine] = dict(error=run.stderr.strip(), exit_code=run.returncode,
                                              failed_block=block)
                        continue
                    measurements = json.loads(timing_path.read_text())
                if not isinstance(measurements, list) or len(measurements) != samples:
                    raise ValueError("unexpected number of workbook samples")
                for sample in measurements:
                    if (not isinstance(sample, dict) or set(sample) != {"import_ns", "calculate_ns"}
                            or any(type(value) is not int or value < 0 for value in sample.values())
                            or sum(sample.values()) == 0):
                        raise ValueError("invalid workbook timing sample")
                result[engine]["samples"].extend(dict(row, block=block) for row in measurements)
            except (OSError, TypeError, ValueError, subprocess.TimeoutExpired) as error:
                result[engine] = dict(error=str(error), failed_block=block)
    return result


def main():
    """Write one report row for every workbook, including excluded timings."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("directory", type=Path)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--samples", type=int, default=5)
    parser.add_argument("--warmup", type=int, default=1)
    parser.add_argument("--blocks", type=int, default=1,
                        help="repeat paired measurements with alternating engine order")
    parser.add_argument("--timeout", type=int, default=120)
    parser.add_argument("--fetch", action="store_true", help="download missing pinned originals")
    parser.add_argument("--oracle-directory", type=Path, help="independently recalculated copies")
    parser.add_argument("--oracle-engine", help="record the exact engine version used for the copies")
    args = parser.parse_args()
    if args.samples < 1 or args.warmup < 0 or args.blocks < 1 or args.timeout < 1:
        parser.error("samples, blocks, and timeout must be positive; warmup must be nonnegative")
    if args.oracle_directory and not args.oracle_engine:
        parser.error("--oracle-directory requires --oracle-engine")
    records = json.loads(args.manifest.read_text())
    binary = args.binary.resolve()
    try:
        binary_sha256 = hashlib.sha256(binary.read_bytes()).hexdigest()
    except OSError as error:
        parser.error(str(error))
    if args.fetch:
        args.directory.mkdir(parents=True, exist_ok=True)
    for index, record in enumerate(records):
        result = run_case(record, args.directory, binary, args.samples, args.warmup,
                          args.timeout, args.fetch, args.oracle_directory, args.oracle_engine,
                          args.blocks, reverse_first=args.blocks > 1 and index % 2 == 1)
        print(json.dumps(dict(result, binary_sha256=binary_sha256)), flush=True)


if __name__ == "__main__":
    main()
