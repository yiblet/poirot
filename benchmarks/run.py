#!/usr/bin/env python3
"""Build isolated engine images and record correctness-gated calculation timings."""
import argparse
from collections import defaultdict
import datetime
import hashlib
import json
import math
from pathlib import Path
import platform
import random
import statistics
import subprocess
import sys

from workloads import generate

ROOT = Path(__file__).resolve().parents[1]
ENGINES = {
    "poirot-evaluation": ("poirot", ["evaluation"], "poirot", "evaluation", "0.1.0"),
    "poirot-overlay": ("poirot", ["overlay"], "poirot", "overlay", "0.1.0"),
    "poirot-evaluation-batch": ("poirot", ["evaluation-batch"], "poirot", "evaluation-batch", "0.1.0"),
    "poirot-overlay-batch": ("poirot", ["overlay-batch"], "poirot", "overlay-batch", "0.1.0"),
    "poirot-workbook-cell": ("poirot", ["workbook-cell"], "poirot", "workbook-cell", "0.1.0"),
    "ironcalc": ("ironcalc", [], "ironcalc", "model", "0.9.0"),
    "hyperformula": ("hyperformula", [], "hyperformula", "automatic", "3.5.0"),
    "hyperformula-array": ("hyperformula", ["array"], "hyperformula", "automatic-array-arithmetic", "3.5.0"),
    "apache-poi": ("apache-poi", [], "apache-poi", "formula-evaluator", "5.5.1"),
    "libreoffice": ("libreoffice", [], "libreoffice", "uno-dirty", "26.8.1.1"),
}


def validate_report(report, suite, selection):
    """Require the exact engine identity and every expected sample, without duplicates."""
    _, _, engine, mode, version = ENGINES[selection]
    if not isinstance(report, dict):
        raise ValueError(f"{selection}: report must be a JSON object")
    if (report.get("schema_version"), report.get("engine"), report.get("mode"), report.get("version")) != (suite["schema_version"], engine, mode, version):
        raise ValueError(f"{selection}: unexpected adapter identity or protocol")
    expected = {(case["name"], "build_calculate_read", sample, None)
                for case in suite["cases"] for sample in range(suite["samples"])}
    expected.update((case["name"], "edit_calculate_read", sample, step)
                    for case in suite["cases"] for sample in range(suite["samples"])
                    for step in range(len(case["edits"])))
    if suite.get("unchanged_read"):
        phase = "warm_evaluate_read" if selection == "poirot-workbook-cell" else "unchanged_read"
        expected.update((case["name"], phase, sample, None)
                        for case in suite["cases"] for sample in range(suite["samples"]))
    actual = set()
    for item in report.get("timings", []):
        key = (item["case"], item["phase"], item["sample"], item["edit"])
        elapsed = item["elapsed_ns"]
        if key not in expected or key in actual or type(elapsed) is not int or elapsed <= 0:
            raise ValueError(f"{selection}: invalid, duplicate, or unexpected timing {item}")
        actual.add(key)
    if actual != expected:
        raise ValueError(f"{selection}: missing {len(expected - actual)} samples")


def summarize(reports):
    """Summarize each phase/edit separately; retain raw measurements in engine JSON."""
    rows = []
    for selection, report in reports.items():
        groups = defaultdict(list)
        for timing in report["timings"]:
            groups[(timing["case"], timing["phase"], timing["edit"])].append(timing["elapsed_ns"])
        for (case, phase, edit), values in sorted(groups.items()):
            values.sort()
            rows.append(dict(engine=selection, case=case, phase=phase, edit=edit,
                             samples=len(values), median_ns=statistics.median(values),
                             p95_ns=values[math.ceil(len(values) * .95) - 1],
                             min_ns=min(values), max_ns=max(values)))
    return rows


def markdown(rows):
    """Produce a compact comparison table without an aggregate winner or speedup claim."""
    lines = ["# Calculation speed results", "",
             "All measurements passed the workload output checks. Times are milliseconds.", "",
             "| Workload | Phase | Edit | Engine | Samples | Median ms | p95 ms |",
             "| --- | --- | --- | --- | ---: | ---: | ---: |"]
    for row in sorted(rows, key=lambda r: (r["case"], r["phase"], -1 if r["edit"] is None else r["edit"], r["engine"])):
        step = "—" if row["edit"] is None else row["edit"] + 1
        lines.append(f'| {row["case"]} | {row["phase"]} | {step} | {row["engine"]} | '
                     f'{row["samples"]} | {row["median_ns"] / 1e6:.4f} | {row["p95_ns"] / 1e6:.4f} |')
    lines += ["", "See metadata.json for hardware, image IDs, source revision, and resource limits.",
              "These synthetic workloads do not establish general Excel compatibility or overall engine rankings.", ""]
    return "\n".join(lines)


def command(args, **kwargs):
    """Run an argument-vector command and propagate failures with stderr preserved."""
    return subprocess.run(args, check=True, text=True, **kwargs)


def source_metadata():
    """Identify the tracked worktree and exact benchmark sources, including new files."""
    commit = command(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True).stdout.strip()
    diff = command(["git", "diff", "HEAD", "--"], cwd=ROOT, capture_output=True).stdout
    status = command(["git", "status", "--porcelain"], cwd=ROOT, capture_output=True).stdout
    hashes = {}
    for path in sorted((ROOT / "benchmarks").rglob("*")):
        relative = path.relative_to(ROOT / "benchmarks")
        if path.is_file() and not any(p in {"target", "node_modules", "results", "__pycache__"} for p in relative.parts):
            hashes[str(relative)] = hashlib.sha256(path.read_bytes()).hexdigest()
    return dict(commit=commit, worktree_status=status, tracked_diff_sha256=hashlib.sha256(diff.encode()).hexdigest(), benchmark_files=hashes)


def cpu_description():
    """Capture the CPU model when Linux exposes it, otherwise the platform description."""
    cpuinfo = Path("/proc/cpuinfo")
    if cpuinfo.exists():
        for line in cpuinfo.read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    return platform.processor()


def main(argv=None):
    """Build images serially, execute comparisons serially, and save auditable results."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engines", nargs="+", choices=ENGINES, default=[name for name in ENGINES if name not in {"poirot-workbook-cell", "hyperformula-array"}])
    parser.add_argument("--size", type=int, default=512)
    parser.add_argument("--samples", type=int, default=15)
    parser.add_argument("--warmup", type=int, default=3)
    parser.add_argument("--seed", type=int, default=42, help="seed for serial engine order")
    parser.add_argument("--cpus", type=float, default=1.0, help="same Docker CPU quota for every engine")
    parser.add_argument("--cpuset-cpus", help="optional host CPU affinity, e.g. 2")
    parser.add_argument("--memory", default="2g", help="same Docker memory/swap limit for every engine")
    parser.add_argument("--timeout", type=int, default=600, help="maximum seconds for each engine process")
    parser.add_argument("--diagnostics", action="store_true", help="include range shape/location probes")
    parser.add_argument("--no-build", action="store_true", help="reuse existing images; IDs are still recorded")
    parser.add_argument("--output", type=Path, help="new result directory (must not already exist)")
    args = parser.parse_args(argv)
    if not math.isfinite(args.cpus) or args.cpus <= 0 or args.timeout <= 0:
        parser.error("cpus and timeout must be positive")
    if args.diagnostics:
        from diagnostics import generate_diagnostics
        suite = generate_diagnostics(args.size, args.samples, args.warmup)
    else:
        suite = generate(args.size, args.samples, args.warmup)
    payload = json.dumps(suite, allow_nan=False, separators=(",", ":"))
    selected = list(dict.fromkeys(args.engines))
    order = selected.copy()
    random.Random(args.seed).shuffle(order)
    output = args.output or ROOT / "benchmarks/results" / datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    (output / "workloads.json").write_text(payload + "\n")
    metadata = dict(schema_version=1, status="running", created_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    platform=platform.platform(), machine=platform.machine(), cpu=cpu_description(),
                    source=source_metadata(), size=args.size, samples=args.samples, warmup=args.warmup,
                    order=order, seed=args.seed, cpus=args.cpus, cpuset_cpus=args.cpuset_cpus,
                    memory=args.memory, timeout=args.timeout, images={}, runtime_versions={},
                    workload_sha256=hashlib.sha256(payload.encode()).hexdigest())
    metadata_path = output / "metadata.json"
    reports = {}
    try:
        metadata["docker"] = json.loads(command(["docker", "version", "--format", "{{json .}}"], capture_output=True).stdout)
        images = list(dict.fromkeys(ENGINES[name][0] for name in selected))
        for image in images:
            tag = f"poirot-bench-{image}:local"
            if not args.no_build:
                print(f"Building {tag}", file=sys.stderr, flush=True)
                command(["docker", "build", "--tag", tag, "--file", f"benchmarks/{image}/Dockerfile", "."], cwd=ROOT, stdout=sys.stderr)
            metadata["images"][image] = json.loads(command(["docker", "image", "inspect", tag], capture_output=True).stdout)[0]
            if image == "hyperformula":
                runtime = command(["docker", "run", "--rm", "--network", "none", "--entrypoint", "node", tag, "--version"], capture_output=True).stdout.strip()
            elif image in {"apache-poi", "libreoffice"}:
                runtime = command(["docker", "run", "--rm", "--network", "none", "--entrypoint", "java", tag, "-version"], capture_output=True).stderr.strip()
            else:
                runtime = command(["docker", "run", "--rm", "--network", "none", "--entrypoint", "cat", tag,
                                   "/licenses/rustc-version.txt"], capture_output=True).stdout.strip()
            metadata["runtime_versions"][image] = runtime
        for name in order:
            image, mode, *_ = ENGINES[name]
            # Execute the inspected image ID, so a moving local tag cannot change the run.
            image_id = metadata["images"][image]["Id"]
            container = "poirot-speed-" + hashlib.sha256(f"{output}:{name}".encode()).hexdigest()[:16]
            cmd = ["docker", "run", "--rm", "--name", container, "-i", "--network", "none", "--read-only",
                   "--tmpfs", "/tmp:rw,size=64m", "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
                   "--pids-limit", "256", "--cpus", str(args.cpus), "--memory", args.memory,
                   "--memory-swap", args.memory]
            if args.cpuset_cpus:
                cmd += ["--cpuset-cpus", args.cpuset_cpus]
            cmd += [image_id, *mode]
            print(f"Running {name}", file=sys.stderr, flush=True)
            try:
                process = subprocess.run(cmd, input=payload, text=True, capture_output=True, timeout=args.timeout)
            except subprocess.TimeoutExpired:
                # Killing the Docker client alone can leave calculation running.
                subprocess.run(["docker", "rm", "--force", container], capture_output=True, timeout=30)
                raise
            (output / f"{name}.stderr.log").write_text(process.stderr)
            if process.returncode:
                raise RuntimeError(f"{name} failed (exit {process.returncode}): {process.stderr[-4000:]}")
            report = json.loads(process.stdout)
            validate_report(report, suite, name)
            reports[name] = report
            (output / f"{name}.json").write_text(json.dumps(report, indent=2) + "\n")
        rows = summarize(reports)
        (output / "summary.json").write_text(json.dumps(rows, indent=2) + "\n")
        (output / "summary.md").write_text(markdown(rows))
        metadata["status"] = "passed"
        print(f"Results: {output / 'summary.md'}")
    except Exception as error:
        metadata["status"] = "failed"
        metadata["error"] = str(error)
        raise
    finally:
        metadata_path.write_text(json.dumps(metadata, indent=2) + "\n")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, RuntimeError, subprocess.SubprocessError, OSError) as error:
        print(f"Benchmark failed: {error}", file=sys.stderr)
        sys.exit(1)
