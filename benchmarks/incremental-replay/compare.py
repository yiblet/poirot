#!/usr/bin/env python3
"""Bounded paired scenario comparison; retain no private input or output contents."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import statistics
import subprocess
import tempfile


def paired_summary(pairs):
    """Return a paired difference interval and two-sided Student t p-value (df=9)."""
    if len(pairs) != 10:
        raise ValueError("statistical summaries require ten independent process pairs")
    differences = [candidate - baseline for baseline, candidate in pairs]
    mean = statistics.mean(differences)
    se = statistics.stdev(differences) / math.sqrt(10)
    half = 2.5738039775468513 * se
    if se == 0:
        p = 1.0 if mean == 0 else 0.0
    else:
        # Integrate the df=9 t density with t=3*tan(theta).
        theta = math.atan(abs(mean / se) / 3)
        sine, cosine = math.sin(theta), math.cos(theta)
        integral = theta
        for power in (2, 4, 6, 8):
            integral = sine * cosine ** (power - 1) / power + (power - 1) * integral / power
        p = max(0.0, min(1.0, 1 - 256 * integral / (35 * math.pi)))
    return dict(baseline=statistics.mean(a for a, _ in pairs),
                candidate=statistics.mean(b for _, b in pairs), difference=mean,
                individual_paired_97pct_ci=[mean - half, mean + half], p=p)


def adjust(rows):
    """Control the family-wise error rate over every reported metric using Holm."""
    largest = 0.0
    for rank, row in enumerate(sorted(rows, key=lambda row: row["p"])):
        largest = max(largest, min(1.0, row["p"] * (len(rows) - rank)))
        row["adjusted_p"] = largest
        row["finding"] = ("inconclusive" if largest >= 0.03 else
                          "faster" if row["difference"] < 0 else "slower")


def run(binary, mode, workload, cpu, limit):
    """Run one independent process and capture aggregate observations and process RSS."""
    with tempfile.TemporaryDirectory(prefix="poirot-incremental-rss-") as temporary:
        usage = Path(temporary) / "usage.txt"
        command = ["/usr/bin/time", "-v", "-o", str(usage), "taskset", "-c", str(cpu),
                   str(binary), mode, *workload, str(limit)]
        process = subprocess.run(command, capture_output=True, text=True,
                                 env=dict(os.environ, LC_ALL="C"))
        if process.returncode:
            # Operational errors may contain private coordinates or formula text.
            raise RuntimeError(f"benchmark process failed with status {process.returncode}; stderr withheld")
        report = json.loads(process.stdout)
        match = re.search(r"Maximum resident set size \(kbytes\): (\d+)", usage.read_text())
        if not match:
            raise RuntimeError("process resource report omitted RSS")
        report["peak_rss_mib"] = int(match.group(1)) / 1024
    records = report["requests"]
    if len(records) != limit or any(record["errors"] for record in records):
        raise RuntimeError("incomplete replay or spreadsheet errors; refusing timing comparison")
    report["signature"] = hashlib.sha256(json.dumps(
        [record["signature"] for record in records], separators=(",", ":")).encode()).hexdigest()
    return report


def metrics(report):
    """Keep first evaluation, later evaluations, request work, load and RSS distinct."""
    requests = report["requests"]
    return {
        "first_evaluation_ms": requests[0]["evaluation_ms"],
        "repeated_evaluation_ms": statistics.mean(row["evaluation_ms"] for row in requests[1:]),
        "mean_request_ms": statistics.mean(sum(row[key] for key in (
            "setup_ms", "edit_ms", "evaluation_ms")) for row in requests),
        "load_ms": report["load_ms"],
        "peak_rss_mib": report["peak_rss_mib"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--workbook", type=Path)
    parser.add_argument("--requests", type=Path)
    parser.add_argument("--limit", type=int, default=100)
    parser.add_argument("--cpu", type=int, default=2)
    parser.add_argument("--exclude-case-mode", action="append", default=[], metavar="CASE:MODE",
                        help="explicitly omit an unsupported case/mode from both versions")
    parser.add_argument("--pairs", type=int, choices=(1, 10), default=10,
                        help="one pair is a correctness smoke run with no significance claims")
    arguments = parser.parse_args()
    if arguments.limit < 2:
        parser.error("limit must be at least two")
    if bool(arguments.workbook) != bool(arguments.requests):
        parser.error("workbook and requests must be supplied together")
    binaries = {name: getattr(arguments, name).resolve() for name in ("baseline", "candidate")}
    workloads = {name: ["--synthetic", name] for name in ("diamond", "branches", "spill")}
    if arguments.workbook:
        workloads["external"] = [str(arguments.workbook), str(arguments.requests)]
    excluded = set(arguments.exclude_case_mode)
    valid = {f"{case}:{mode}" for case in workloads for mode in ("persistent", "reset", "fresh")}
    if not excluded <= valid:
        parser.error("excluded case/mode does not exist")
    arguments.output.mkdir(parents=True, exist_ok=False)
    metadata = dict(pairs=arguments.pairs, requests_per_process=arguments.limit,
                    cpu=arguments.cpu, modes=["persistent", "reset", "fresh"],
                    excluded_case_modes=sorted(excluded),
                    independent_unit="process pair; requests within a process are correlated",
                    intervals="individual paired 97%; Holm correction across all metric comparisons",
                    binaries={name: hashlib.sha256(path.read_bytes()).hexdigest()
                              for name, path in binaries.items()})
    (arguments.output / "metadata.json").write_text(json.dumps(metadata, indent=2))
    rows = []
    observations = {}
    for trial in range(arguments.pairs):
        for workload, command in workloads.items():
            signatures = set()
            # Rotate mode order to reduce a fixed relationship with thermal drift.
            modes = metadata["modes"]
            offset = trial % len(modes)
            for mode in modes[offset:] + modes[:offset]:
                if f"{workload}:{mode}" in excluded:
                    continue
                versions = list(binaries) if trial % 2 == 0 else list(reversed(binaries))
                for version in versions:
                    report = run(binaries[version], mode, command, arguments.cpu, arguments.limit)
                    signatures.add(report["signature"])
                    observations[(trial, workload, mode, version)] = metrics(report)
                    (arguments.output / f"{trial:02}-{workload}-{mode}-{version}.json").write_text(
                        json.dumps(report, indent=2))
            if len(signatures) != 1:
                raise RuntimeError(f"output hashes disagree between versions or fresh/reset/persistent modes: {workload}")
        print(f"matched process pair {trial + 1}/{arguments.pairs}", flush=True)
    if arguments.pairs == 10:
        for workload in workloads:
            for mode in metadata["modes"]:
                if f"{workload}:{mode}" in excluded:
                    continue
                for metric in metrics(report):
                    pairs = [(observations[(trial, workload, mode, "baseline")][metric],
                              observations[(trial, workload, mode, "candidate")][metric])
                             for trial in range(10)]
                    rows.append(dict(workload=workload, mode=mode, metric=metric,
                                     **paired_summary(pairs)))
        adjust(rows)
    (arguments.output / "summary.json").write_text(json.dumps(rows, indent=2))
    print(json.dumps(rows, indent=2))


if __name__ == "__main__":
    main()
