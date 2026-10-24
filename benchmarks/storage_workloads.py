#!/usr/bin/env python3
"""Emit optional original storage-layout and dynamic-spill evaluation controls."""
import argparse
import json
import math

from workloads import address, case, cell, edit, generate


def column_name(col):
    """Render a zero-based bounded column as an Excel reference."""
    result = ""
    col += 1
    while col:
        col, digit = divmod(col - 1, 26)
        result = chr(65 + digit) + result
    return result


def generate_storage(size=1024, samples=3, warmup=2, include_base=True):
    """Keep arithmetic identical while varying physical layout and spill topology."""
    suite = generate(size, samples, warmup)  # Also validates public bounds.
    if not include_base:
        suite["cases"] = []
    suite.update(schema_version=2, unchanged_read=True)
    width = math.isqrt(size - 1) + 1
    for layout in ["tall", "wide", "rectangle", "checkerboard", "scattered", "sheets"]:
        cells, outputs = [], []
        for index in range(size):
            sheet = 0
            if layout == "tall":
                row, col = index, 0
            elif layout == "wide":
                row, col = 0, index * 2
            elif layout == "rectangle":
                row, col = index // width, index % width * 2
            elif layout == "checkerboard":
                row, col = index // width * 2, index % width * 4
            elif layout == "scattered":
                row, col = index * 31, index * 194 % 4096
            else:
                sheet, row, col = index % 32, index // 32, 0
            literal = cell(row, col, index + 1)
            formula = cell(row, col + 1, f"={column_name(col)}{row + 1}*2+1")
            literal["sheet"] = formula["sheet"] = sheet
            cells.extend([literal, formula])
            outputs.append(dict(address(row, col + 1), sheet=sheet))
        # Independent affine result: input i+1 yields 2i+3. Only input0 changes.
        expected = [2 * index + 3 for index in range(size)]
        changes = [dict(edit(0, 0, value, [2 * value + 1] + expected[1:]), sheet=0)
                   for value in [2, 3]]
        workload = case("layout_" + layout, "Same affine work under different coordinates.",
                        cells, outputs, expected, changes)
        if layout == "sheets":
            workload["sheet_count"] = min(32, size)
        suite["cases"].append(workload)

    # SEQUENCE enumerates row-major: lower-right of a 2x2 block is start+3.
    n = min(size, 1024)
    suite["cases"].append(case(
        "many_spill_producers", "Independent 2x2 arrays, read lower-right children.",
        [cell(0, 7, 1)] + [cell(3 * r, 0, f"=SEQUENCE(2,2,$H$1+{r})") for r in range(n)],
        [address(3 * r + 1, 1) for r in range(n)], [r + 4 for r in range(n)],
        [edit(0, 7, value, [r + value + 3 for r in range(n)]) for value in [2, 3]]))
    suite["cases"].append(case(
        "large_spill", "One four-column rectangle, read every element.",
        [cell(0, 7, 1), cell(0, 0, f"=SEQUENCE({size},4,$H$1)")],
        [address(r, c) for r in range(size) for c in range(4)], list(range(1, 4 * size + 1)),
        [edit(0, 7, value, list(range(value, 4 * size + value))) for value in [2, 3]]))
    height = min(size, 32)
    suite["cases"].append(case(
        "scalar_array_transition", "Switch between scalar and rectangular array, reading former child.",
        [cell(0, 7, 0), cell(0, 0, f"=IF($H$1=1,SEQUENCE({height},4),7)"),
         cell(0, 5, f"=A{height}+1")], [address(0, 0), address(0, 5)], [7, 1],
        [edit(0, 7, 1, [1, 4 * (height - 1) + 2]), edit(0, 7, 2, [7, 1])]))
    return suite


def main():
    """Write protocol JSON; generation is outside adapter timing."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--size", type=int, default=1024)
    parser.add_argument("--samples", type=int, default=3)
    parser.add_argument("--warmup", type=int, default=2)
    parser.add_argument("--without-base", action="store_true")
    args = parser.parse_args()
    print(json.dumps(generate_storage(args.size, args.samples, args.warmup,
                                      not args.without_base), allow_nan=False))


if __name__ == "__main__":
    main()
