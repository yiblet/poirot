#!/usr/bin/env python3
"""Compare frozen storage diagnostics using a bounded number of process pairs."""
import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import subprocess


def shared_statistics():
    """Use the project's existing paired t-test and Holm implementation."""
    path = Path(__file__).resolve().parents[1] / "incremental-replay" / "compare.py"
    spec = importlib.util.spec_from_file_location("incremental_statistics", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def validate_report(report):
    """Reject incomplete reports and invalid timings before statistical analysis."""
    workloads = report.get("workloads") if isinstance(report, dict) else None
    if not isinstance(workloads, dict) or not workloads:
        raise ValueError("storage report has no workloads")
    for metrics in workloads.values():
        if not isinstance(metrics, dict) or type(metrics.get("cells")) is not int or metrics["cells"] <= 0:
            raise ValueError("storage report has no valid coordinate count")
        timings = [value for key, value in metrics.items() if key.endswith("_ns")]
        if not timings or any(type(value) not in (int, float) or not math.isfinite(value) or value <= 0
                              for value in timings):
            raise ValueError("storage report contains invalid timing")
    return workloads


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--cpu", type=int, default=2)
    parser.add_argument("--pairs", type=int, choices=(1, 10), default=10)
    args = parser.parse_args()
    binaries = {name: getattr(args, name).resolve() for name in ("baseline", "candidate")}
    args.output.mkdir(parents=True, exist_ok=False)
    metadata = dict(pairs=args.pairs, cpu=args.cpu,
                    binaries={name: hashlib.sha256(path.read_bytes()).hexdigest()
                              for name, path in binaries.items()},
                    independent_unit="process pair", intervals="paired 97%; Holm across timing metrics")
    (args.output / "metadata.json").write_text(json.dumps(metadata, indent=2))
    observations = []
    for trial in range(args.pairs):
        reports = {}
        order = list(binaries) if trial % 2 == 0 else list(reversed(binaries))
        for name in order:
            process = subprocess.run(["taskset", "-c", str(args.cpu), str(binaries[name])],
                                     check=True, capture_output=True, text=True)
            reports[name] = validate_report(json.loads(process.stdout))
        if reports["baseline"].keys() != reports["candidate"].keys():
            raise RuntimeError("workload membership differs")
        for name, baseline in reports["baseline"].items():
            candidate = reports["candidate"][name]
            if baseline.keys() != candidate.keys() or baseline["cells"] != candidate["cells"]:
                raise RuntimeError("metric or coordinate counts differ")
        observations.append(reports)
        (args.output / f"{trial:02}.json").write_text(json.dumps(reports, indent=2))
        print(f"paired process {trial + 1}/{args.pairs}", flush=True)
    if args.pairs == 1:
        return
    statistics = shared_statistics()
    rows = []
    for workload, report in observations[0]["baseline"].items():
        for metric in report:
            if metric.endswith("_ns"):
                pairs = [(row["baseline"][workload][metric], row["candidate"][workload][metric])
                         for row in observations]
                rows.append(dict(workload=workload, metric=metric, **statistics.paired_summary(pairs)))
    statistics.adjust(rows)
    (args.output / "summary.json").write_text(json.dumps(rows, indent=2))


if __name__ == "__main__":
    main()
