#!/usr/bin/env python3
"""Emit correctness-gated scaling probes in the standard adapter input protocol."""
import argparse
import json

from workloads import address, case, cell, edit, generate


def generate_diagnostics(size=512, samples=15, warmup=3):
    """Separate output publication, range length, and range position costs."""
    suite = generate(size, samples, warmup)
    # Each formula reads four literals: total range work grows linearly, while
    # preceding-anchor spill discovery can still grow with the sheet position.
    cells = []
    expected = []
    outputs = []
    for index in range(size):
        start = 4 * index
        cells.extend(cell(start + offset, 0, offset + 1) for offset in range(4))
        cells.append(cell(start + 3, 1, f"=SUM(A{start + 1}:A{start + 4})"))
        outputs.append(address(start + 3, 1))
        expected.append(10)
    suite["cases"].append(case(
        "fixed_disjoint_ranges", "Four-cell disjoint ranges at increasing rows.",
        cells, outputs, expected,
        [edit(0, 0, 2, [11] + expected[1:]), edit(0, 0, 3, [12] + expected[1:])]))
    # Same range length/output count; only the physical location differs.
    for name, row in [("fixed_shared_top", 0), ("fixed_shared_bottom", 4 * size)]:
        cells = [cell(row + offset, 0, offset + 1) for offset in range(4)]
        cells.extend(cell(index, 1, f"=SUM($A${row + 1}:$A${row + 4})")
                     for index in range(size))
        suite["cases"].append(case(
            name, "Repeated four-cell range; top/bottom location is the only change.",
            cells, [address(index, 1) for index in range(size)], [10] * size,
            [edit(row, 0, 2, [11] * size), edit(row, 0, 3, [12] * size)]))
    return suite


def main():
    """Print a suite for piping into any engine adapter, without timing generation."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--size", type=int, default=512)
    parser.add_argument("--samples", type=int, default=15)
    parser.add_argument("--warmup", type=int, default=3)
    args = parser.parse_args()
    print(json.dumps(generate_diagnostics(args.size, args.samples, args.warmup), allow_nan=False))


if __name__ == "__main__":
    main()
