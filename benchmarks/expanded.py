#!/usr/bin/env python3
"""Run expanded Docker coverage; preserve capability gaps and mismatches explicitly."""
import argparse
import collections
import datetime
import hashlib
import json
import math
from pathlib import Path
import platform
import random
import subprocess
from types import SimpleNamespace
import sys
import time

from coverage import generate_coverage
from run import ENGINES, ROOT, command, cpu_description, markdown, source_metadata, summarize, validate_report

# Reviewed feature exclusions, not a catch-all for failures. The positive probe
# still runs, and must fail in the documented way before any case is excluded.
CAPABILITY_GAPS = {
    ("apache-poi", "dynamic_arrays"): dict(markers=["NotImplementedFunctionException: SEQUENCE"],
        reason="POI 5.5.1 FormulaEvaluator explicitly does not implement SEQUENCE"),
    ("apache-poi", "dynamic_array_dimensions"): dict(markers=["NotImplementedFunctionException: SEQUENCE"],
        reason="POI 5.5.1 FormulaEvaluator explicitly does not implement SEQUENCE"),
    ("apache-poi", "modern_variance"): dict(markers=["Dotted range (full row or column) expression 'VAR.P' must have exactly 2 dots"],
        reason="POI 5.5.1 parser rejects the modern VAR.P spelling; legacy VARP is a separate case"),
    ("libreoffice", "dynamic_arrays"): dict(markers=["nonnumeric Calc formula result at row=1,col=1: \n"],
        reason="Calc 26.8.1.1 setTokens workflow computes the SEQUENCE anchor but leaves its child blank"),
    ("libreoffice", "dynamic_array_dimensions"): dict(markers=["nonnumeric Calc formula result at row=1,col=1: \n"],
        reason="Calc 26.8.1.1 setTokens workflow computes the SEQUENCE anchor but leaves its child blank"),
    ("hyperformula-array", "dynamic_array_dimensions"): dict(markers=['capability_dynamic_array_dimensions: output 0: expected 1, got {"value":"#VALUE!","type":"VALUE","message":"Cell range not allowed."}'],
        reason="HyperFormula 3.5.0 rejects input-sized SEQUENCE dimensions even with array arithmetic enabled"),
}


def capability_probe(feature):
    """Exercise the feature itself without wrapping an error as a valid number."""
    if feature == "exact_lookup":
        return dict(name="capability_exact_lookup", cells=[
            dict(row=0, col=0, input=1), dict(row=0, col=1, input=3),
            dict(row=1, col=0, input=2), dict(row=1, col=1, input=6),
            dict(row=0, col=3, input="=VLOOKUP(2,A1:B2,2,0)")],
            outputs=[dict(row=0, col=3)], expected=[6], edits=[])
    if feature == "modern_variance":
        return dict(name="capability_modern_variance", cells=[dict(row=0, col=0, input="=VAR.P(1,2,3,4)")],
                    outputs=[dict(row=0, col=0)], expected=[1.25], edits=[])
    if feature not in ["dynamic_arrays", "dynamic_array_dimensions"]:
        raise ValueError(f"unknown capability {feature}")
    cells = [dict(row=0, col=0, input="=SEQUENCE(2,2)" if feature == "dynamic_arrays" else "=SEQUENCE(G1,2)")]
    if feature == "dynamic_array_dimensions":
        cells.append(dict(row=0, col=6, input=2))
    return dict(name="capability_" + feature, description="Read a real SEQUENCE anchor and spill child.",
                cells=cells, outputs=[dict(row=0, col=0), dict(row=1, col=1)], expected=[1, 4], edits=[])


def validate_suite(suite):
    """Reject malformed inputs and unversioned features before invoking any engine."""
    schema = suite.get("schema_version")
    if type(schema) is not int or schema not in [1, 2] or not suite.get("cases"):
        raise ValueError("unsupported or empty suite")
    if schema == 1 and suite.get("unchanged_read"):
        raise ValueError("unchanged_read requires schema2")
    names = set()
    for case in suite["cases"]:
        if case["name"] in names:
            raise ValueError("duplicate case name")
        names.add(case["name"])
        count = case.get("sheet_count", 1)
        if type(count) is not int or count < 1 or (schema == 1 and count != 1):
            raise ValueError("invalid sheet count or feature revision")
        for kind in ["cells", "outputs", "edits"]:
            for item in case[kind]:
                for key, bound in [("sheet", count), ("row", 1048576), ("col", 16384)]:
                    value = item.get(key, 0) if key == "sheet" else item[key]
                    if type(value) is not int or not 0 <= value < bound:
                        raise ValueError("coordinate outside workbook")
                if kind == "outputs":
                    continue
                value = item["input" if kind == "cells" else "value"]
                number = type(value) in [int, float] and math.isfinite(value)
                formula = type(value) is str and value.startswith("=") and len(value) > 1
                text = schema == 2 and type(value) is dict and set(value) == {"text"} and type(value["text"]) is str
                if not (number or formula or text) or (schema == 1 and kind == "edits" and not number):
                    raise ValueError("invalid or unversioned source input")


def outcome_kind(engine, feature, stderr, returncode):
    """Recognize only specifically reviewed capability failures; everything else fails."""
    evidence = CAPABILITY_GAPS.get((engine, feature))
    if returncode > 0 and evidence and any(marker in stderr for marker in evidence["markers"]):
        return "unsupported", evidence["reason"]
    return "mismatch", "unexpected error, wrong result, or protocol failure"


def docker_command(selection, image_id, args, label):
    """Build one resource-matched isolated invocation, optionally using frozen Rust bins."""
    image, mode, *_ = ENGINES[selection]
    container = "poirot-coverage-" + hashlib.sha256(label.encode()).hexdigest()[:16]
    cmd = ["docker", "run", "--rm", "--name", container, "-i", "--network", "none", "--read-only",
           "--tmpfs", "/tmp:rw,size=64m", "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
           "--pids-limit", "256", "--cpus", str(args.cpus), "--memory", args.memory, "--memory-swap", args.memory]
    if args.cpuset_cpus:
        cmd += ["--cpuset-cpus", args.cpuset_cpus]
    if args.rust_bin_dir and image in ["poirot", "ironcalc"]:
        cmd += ["--mount", f"type=bind,source={args.rust_bin_dir.resolve()},target=/bench,readonly",
                "--entrypoint", f"/bench/{image}-speed"]
    return cmd + [image_id, *mode], container


def execute(selection, suite, args, images, output, label):
    """Keep stdout/stderr even on failure; only exact complete reports count as success."""
    validate_suite(suite)
    image = ENGINES[selection][0]
    cmd, container = docker_command(selection, images[image]["Id"], args, str(output) + label)
    (output / (label + ".workloads.json")).write_text(json.dumps(suite, allow_nan=False))
    started = time.time_ns()
    try:
        process = subprocess.run(cmd, input=json.dumps(suite, allow_nan=False), text=True, capture_output=True, timeout=args.timeout)
    except subprocess.TimeoutExpired as error:
        cleanup_error = ""
        try:
            subprocess.run(["docker", "rm", "--force", container], capture_output=True, timeout=30)
        except (subprocess.SubprocessError, OSError) as cleanup:
            cleanup_error = "\nContainer cleanup failed: " + str(cleanup)
        def decoded(value):
            return value.decode(errors="replace") if isinstance(value, bytes) else value or ""
        process = SimpleNamespace(returncode=-1, stdout=decoded(error.stdout),
                                  stderr=decoded(error.stderr) + "\n" + str(error) + cleanup_error)
    events_path = output / "invocations.json"
    events = json.loads(events_path.read_text()) if events_path.exists() else []
    events.append(dict(label=label, command=cmd, started_unix_ns=started, ended_unix_ns=time.time_ns(), returncode=process.returncode))
    events_path.write_text(json.dumps(events, indent=2))
    (output / (label + ".stderr.log")).write_text(process.stderr)
    (output / (label + ".stdout.json")).write_text(process.stdout)
    if process.returncode:
        return None, process.stderr, process.returncode, cmd
    try:
        report = json.loads(process.stdout)
        validate_report(report, suite, selection)
    except (ValueError, KeyError, TypeError) as error:
        return None, "protocol failure: " + str(error), -2, cmd
    return report, process.stderr, 0, cmd


def run_group(selection, suite, args, images, output, label, records, reports):
    """Bisect failed groups to retain each failing case, never mislabeling good siblings."""
    report, error, code, cmd = execute(selection, suite, args, images, output, label)
    if report is not None:
        reports.append((args.current_size, selection, report))
        records.extend(dict(engine=selection, size=args.current_size, case=c["name"], status="validated",
                            evidence=label, command=cmd) for c in suite["cases"])
        return
    if len(suite["cases"]) > 1:
        midpoint = len(suite["cases"]) // 2
        for suffix, cases in [("a", suite["cases"][:midpoint]), ("b", suite["cases"][midpoint:])]:
            run_group(selection, dict(suite, cases=cases), args, images, output, label + suffix, records, reports)
        return
    records.append(dict(engine=selection, size=args.current_size, case=suite["cases"][0]["name"],
                        status="mismatch", reason=error[-4000:], returncode=code, evidence=label, command=cmd))


def comparison_rows(rows):
    """Describe paired-workload median ratios without an inferential claim."""
    indexed = {(r["engine"], r["size"], r["case"], r["phase"], r["edit"]): r for r in rows}
    result = []
    for row in rows:
        reference = indexed.get(("ironcalc", row["size"], row["case"], row["phase"], row["edit"]))
        if reference and row["engine"] != "ironcalc":
            result.append(dict(size=row["size"], case=row["case"], phase=row["phase"], edit=row["edit"],
                engine=row["engine"], median_ns=row["median_ns"], ironcalc_median_ns=reference["median_ns"],
                descriptive_ratio=row["median_ns"] / reference["median_ns"]))
    return result


def scaling_rows(rows):
    """Retain adjacent-size timing growth; this is screening, not an asymptotic proof."""
    groups = collections.defaultdict(list)
    for row in rows:
        groups[row["engine"], row["case"], row["phase"], row["edit"]].append(row)
    result = []
    for (engine, case, phase, edit), group in groups.items():
        group.sort(key=lambda row: row["size"])
        for lower, upper in zip(group, group[1:]):
            growth = upper["median_ns"] / lower["median_ns"]
            size_ratio = upper["size"] / lower["size"]
            result.append(dict(engine=engine, case=case, phase=phase, edit=edit,
                lower_size=lower["size"], upper_size=upper["size"], lower_median_ns=lower["median_ns"],
                upper_median_ns=upper["median_ns"], time_growth=growth, size_growth=size_ratio,
                descriptive_exponent=math.log(growth) / math.log(size_ratio)))
    return result


def main(argv=None):
    """Screen useful sizes once; process samples are not independent statistical trials."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engines", nargs="+", choices=ENGINES,
                        default=["poirot-evaluation", "poirot-overlay", "ironcalc", "hyperformula-array", "apache-poi", "libreoffice"])
    parser.add_argument("--sizes", nargs="+", type=int, default=[16, 64, 256])
    parser.add_argument("--cases", nargs="+", help="explicit subset; omitted means all original and expanded cases")
    parser.add_argument("--samples", type=int, default=3)
    parser.add_argument("--warmup", type=int, default=1)
    parser.add_argument("--seed", type=int, default=42, help="engine order only, not workbook data")
    parser.add_argument("--cpus", type=float, default=1)
    parser.add_argument("--cpuset-cpus")
    parser.add_argument("--memory", default="2g")
    parser.add_argument("--timeout", type=int, default=600)
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--image-suffix", default="local")
    parser.add_argument("--rust-bin-dir", type=Path, help="read-only frozen host release binaries plus rustc-version.txt; recorded hashes")
    parser.add_argument("--rust-runtime-image", default="poirot-bench-ironcalc:local", help="shared compatible image when mounting host Rust binaries")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    if not math.isfinite(args.cpus) or args.cpus <= 0 or args.timeout <= 0:
        parser.error("cpus and timeout must be positive")
    if len(set(args.sizes)) != len(args.sizes) or any(not 4 <= size <= 5000 for size in args.sizes):
        parser.error("sizes must be distinct values in 4..5000")
    selected = list(dict.fromkeys(args.engines))
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    metadata = dict(schema_version=2, status="running", source=source_metadata(), cpu=cpu_description(),
                    platform=platform.platform(), machine=platform.machine(),
                    cpus=args.cpus, cpuset_cpus=args.cpuset_cpus, memory=args.memory, seed=args.seed,
                    samples=args.samples, warmup=args.warmup, sizes=args.sizes,
                    created_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    sampling="exploratory screen; within-process samples are correlated; no significance or parity claim")
    records, reports, images = [], [], {}
    try:
        metadata["docker"] = json.loads(command(["docker", "version", "--format", "{{json .}}"], capture_output=True).stdout)
        if args.rust_bin_dir:
            paths = [args.rust_bin_dir / file for file in ["poirot-speed", "ironcalc-speed", "rustc-version.txt"]]
            metadata["frozen_rust"] = {str(path.resolve()): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}
            metadata["rustc"] = paths[-1].read_text().strip()
        for image in dict.fromkeys(ENGINES[name][0] for name in selected):
            tag = args.rust_runtime_image if args.rust_bin_dir and image in ["poirot", "ironcalc"] else f"poirot-bench-{image}:{args.image_suffix}"
            if not args.no_build and not (args.rust_bin_dir and image in ["poirot", "ironcalc"]):
                command(["docker", "build", "--tag", tag, "--file", f"benchmarks/{image}/Dockerfile", "."], cwd=ROOT, stdout=sys.stderr)
            images[image] = json.loads(command(["docker", "image", "inspect", tag], capture_output=True).stdout)[0]
        metadata["images"] = images
        metadata["runtime_versions"] = {}
        for image, info in images.items():
            if image in ["poirot", "ironcalc"] and args.rust_bin_dir:
                metadata["runtime_versions"][image] = metadata["rustc"] + " (mounted host binaries)"
            else:
                entry, arguments = ("node", ["--version"]) if image == "hyperformula" else ("java", ["-version"]) if image in ["apache-poi", "libreoffice"] else ("cat", ["/licenses/rustc-version.txt"])
                runtime = command(["docker", "run", "--rm", "--network", "none", "--entrypoint", entry, info["Id"], *arguments], capture_output=True)
                metadata["runtime_versions"][image] = (runtime.stdout + runtime.stderr).strip()
        rust_versions = {metadata["runtime_versions"][image] for image in images if image in ["poirot", "ironcalc"]}
        if len(rust_versions) > 1:
            raise ValueError("Rust engines must use the same compiler")
        order = selected.copy()
        random.Random(args.seed).shuffle(order)
        metadata["order"] = order
        for size in args.sizes:
            args.current_size = size
            suite = generate_coverage(size, args.samples, args.warmup)
            if args.cases:
                unknown = set(args.cases) - {c["name"] for c in suite["cases"]}
                if unknown:
                    raise ValueError(f"unknown cases: {sorted(unknown)}")
                suite["cases"] = [c for c in suite["cases"] if c["name"] in args.cases]
            for selection in order:
                cases = suite["cases"]
                for feature in sorted({f for c in cases for f in c.get("capabilities", [])}):
                    probe = dict(suite, cases=[capability_probe(feature)], samples=1, warmup=0, unchanged_read=False)
                    label = f"{size}-{selection}-probe-{feature}"
                    report, error, code, cmd = execute(selection, probe, args, images, output, label)
                    if report is None:
                        status, reason = outcome_kind(selection, feature, error, code)
                        affected = [c for c in cases if feature in c.get("capabilities", [])]
                        records.extend(dict(engine=selection, size=size, case=c["name"], status=status, reason=reason,
                                            diagnostic=error[-4000:], evidence=label, command=cmd) for c in affected)
                        cases = [c for c in cases if c not in affected]
                if cases:
                    run_group(selection, dict(suite, cases=cases), args, images, output,
                              f"{size}-{selection}", records, reports)
                print(size, selection, collections.Counter(r["status"] for r in records if r["size"] == size and r["engine"] == selection), flush=True)
        rows = []
        for size, selection, report in reports:
            rows.extend(dict(size=size, **row) for row in summarize({selection: report}))
        (output / "summary.json").write_text(json.dumps(rows, indent=2))
        comparisons = comparison_rows(rows)
        scaling = scaling_rows(rows)
        (output / "comparisons.json").write_text(json.dumps(comparisons, indent=2))
        (output / "scaling.json").write_text(json.dumps(scaling, indent=2))
        failures = [row for row in records if row["status"] == "mismatch"]
        lines = ["# Expanded coverage results", "",
                 f"{sum(r['status'] == 'validated' for r in records)} validated engine/case outcomes; "
                 f"{sum(r['status'] == 'unsupported' for r in records)} unsupported; {len(failures)} mismatches.",
                 "Mismatches make this command fail and contribute no successful timings. "
                 "All timing ratios are exploratory; no significance or parity claim is made.", ""]
        for failure in failures:
            lines.append(f"- Size {failure['size']}, {failure['engine']}, {failure['case']}: see {failure['evidence']}.stderr.log")
        lines.append("")
        for size in args.sizes:
            lines += [f"# Size {size}", "", markdown([r for r in rows if r["size"] == size])]
        lines += ["## Capability and mismatch outcomes", "", "| Size | Engine | Case | Status | Reason |", "| ---: | --- | --- | --- | --- |"]
        for row in records:
            if row["status"] != "validated":
                reason = row["reason"].replace("\n", " ").replace("|", "/")[:300]
                lines.append(f'| {row["size"]} | {row["engine"]} | {row["case"]} | {row["status"]} | {reason} |')
        lines += ["", "## Descriptive comparisons with IronCalc", "", "Ratios compare process medians; no significance or parity claim.", "",
                  "| Size | Case | Phase | Edit | Engine | Engine / IronCalc |", "| ---: | --- | --- | --- | --- | ---: |"]
        lines += [f'| {r["size"]} | {r["case"]} | {r["phase"]} | {r["edit"]} | {r["engine"]} | {r["descriptive_ratio"]:.3f} |' for r in comparisons]
        lines += ["", "## Adjacent-size growth", "", "| Case | Phase | Edit | Engine | Sizes | Time growth | Size growth |", "| --- | --- | --- | --- | --- | ---: | ---: |"]
        lines += [f'| {r["case"]} | {r["phase"]} | {r["edit"]} | {r["engine"]} | {r["lower_size"]} to {r["upper_size"]} | {r["time_growth"]:.3f} | {r["size_growth"]:.3f} |' for r in scaling]
        (output / "summary.md").write_text("\n".join(lines))
        metadata["status"] = "failed" if any(r["status"] == "mismatch" for r in records) else "passed_with_explicit_capabilities"
    except Exception as error:
        metadata.update(status="failed", error=str(error))
        raise
    finally:
        (output / "outcomes.json").write_text(json.dumps(records, indent=2))
        (output / "metadata.json").write_text(json.dumps(metadata, indent=2))
    return 1 if metadata["status"] == "failed" else 0


if __name__ == "__main__":
    sys.exit(main())
