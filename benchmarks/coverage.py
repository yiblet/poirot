"""Original, deterministic coverage cases with independently computed numeric results."""
import argparse
import datetime
import json

from diagnostics import generate_diagnostics
from workloads import address, case, cell, edit


def column(index):
    """Render a zero-based Excel column for horizontal neighbors."""
    result = ""
    index += 1
    while index:
        index, digit = divmod(index - 1, 26)
        result = chr(65 + digit) + result
    return result


def neighbors(size, samples=3, warmup=1):
    """Reproduce all eleven accepted range neighbors without loading result fixtures."""
    suite = generate_diagnostics(size, samples, warmup)
    n = size
    cells = []
    for i in range(n):
        cells += [cell(4 * i + j, 0, f"={j + 1}*1") for j in range(4)]
        cells.append(cell(4 * i + 3, 1, f"=SUM(A{4 * i + 1}:A{4 * i + 4})"))
    suite["cases"].append(case("fixed_disjoint_formula_ranges",
        "Four-cell disjoint ranges over authored scalar formulas.", cells,
        [address(4 * i + 3, 1) for i in range(n)], [10] * n,
        [edit(0, 0, value, [9 + value] + [10] * (n - 1)) for value in [2, 3]]))
    description = "Original structural range neighbor; expectations from integer arithmetic and reference SUM ignores text/boolean/blank."
    for name in ["horizontal", "rectangular", "identical", "formula_members",
                 "mixed_members", "alternating_operations", "descending"]:
        cells = []
        for i in range(n):
            r = i + 1
            if name == "horizontal":
                cells.append(cell(0, i, r))
            elif name == "rectangular":
                cells += [cell(i, 0, r), cell(i, 1, 1)]
            elif name == "formula_members":
                cells += [cell(i, 2, r), cell(i, 0, f"=C{r}*2")]
            elif name == "mixed_members":
                if i % 4 != 3:
                    cells.append(cell(i, 0, [r, "=TRUE()", '="ignored"'][i % 4]))
            else:
                cells.append(cell(i, 0, r))
        outputs, expected = [], []
        for i in range(n):
            r = n if name == "identical" else n - i if name == "descending" else i + 1
            operation = "SUMSQ" if name == "alternating_operations" and i % 2 else "SUM"
            endpoint = f"{column(r - 1)}1" if name == "horizontal" else f"B{r}" if name == "rectangular" else f"A{r}"
            at = address(2, i) if name == "horizontal" else address(i, 3)
            cells.append(cell(at["row"], at["col"], f"={operation}(A1:{endpoint})"))
            outputs.append(at)
            numbers = [j + 1 for j in range(r) if name != "mixed_members" or j % 4 == 0]
            value = sum(v * v if operation == "SUMSQ" else v for v in numbers)
            expected.append(value * 2 if name == "formula_members" else value + r if name == "rectangular" else value)
        edits = []
        for value in [2, 3]:
            edits.append(edit(0, 2 if name == "formula_members" else 0, value,
                [v + (value - 1) * 2 if name == "formula_members" else
                 v + value * value - 1 if name == "alternating_operations" and i % 2 else
                 v + value - 1 for i, v in enumerate(expected)]))
        suite["cases"].append(case("reuse_" + name, description, cells, outputs, expected, edits))
    return suite


def generate_coverage(size=64, samples=3, warmup=1):
    """Add cost families to unchanged original gates; schema2 enables honest warm reads."""
    suite = neighbors(size, samples, warmup)
    suite.update(schema_version=2, unchanged_read=True)
    n = size
    outputs = [address(i, 3) for i in range(n)]

    def add(name, family, description, cells, expected, edits, out=None, **metadata):
        """Attach inspectable family and capability metadata without changing the protocol."""
        item = case(name, description, cells, outputs if out is None else out, expected, edits)
        item.update(family=family, **metadata)
        suite["cases"].append(item)

    # Each branch joins two paths from one source. Single/many outputs differ only
    # in the observable boundary, not the graph being calculated.
    for many in [False, True]:
        cells = [cell(0, 0, 1)]
        for i in range(n):
            cells += [cell(i, 1, f"=$A$1+{i}"), cell(i, 2, f"=$A$1*2+{i}"),
                      cell(i, 3, f"=B{i + 1}+C{i + 1}")]
        values = [3 + 2 * i for i in range(n)]
        if not many:
            cells.append(cell(0, 4, f"=SUM(D1:D{n})"))
        observed = values if many else [sum(values)]
        edits = [edit(0, 0, v, [x + 3 * (v - 1) for x in values] if many else
                      [sum(values) + 3 * n * (v - 1)]) for v in [2, 3]]
        add("diamond_many" if many else "diamond_single", "graph", "Diamond joins with shared ancestors.",
            cells, observed, edits, out=None if many else [address(0, 4)])
    depth = n  # Deliberately uncapped; operational depth refusals are visible.
    cells = [cell(0, 0, 1), cell(0, 1, "=A1+1")]
    cells += [cell(i, 1, f"=B{i}+1") for i in range(1, depth)]
    add("long_chain", "graph", "Uncapped dependency chain; report native resource refusals.",
        cells, [depth + 1], [edit(0, 0, v, [depth + v]) for v in [2, 3]],
        out=[address(depth - 1, 1)])
    # Sparse anti-diagonal outputs grow incomparable demand rectangles.
    for layout in ["antidiagonal", "tall_then_wide"]:
        coords = [(i, n - i) for i in range(n)] if layout == "antidiagonal" else [(i, 1) for i in range(n)] + [(0, n)]
        cells = [cell(0, 0, 1)] + [cell(r, c, "=$A$1+1") for r, c in coords]
        add("sparse_" + layout, "discovery", "Sparse demands with distinct spatial completeness costs.",
            cells, [2] * len(coords), [edit(0, 0, v, [v + 1] * len(coords)) for v in [2, 3]],
            out=[address(r, c) for r, c in coords])

    width = max(2, n // 4)
    for rectangular in [False, True]:
        cells = [cell(i, 0, i + 1) for i in range(n + width)]
        if rectangular:
            cells += [cell(i, 1, 2) for i in range(n + width)]
        endcol = "B" if rectangular else "A"
        cells += [cell(i, 3, f"=SUM(A{i + 1}:{endcol}{i + width})") for i in range(n)]
        values = [sum(range(i + 1, i + width + 1)) + (2 * width if rectangular else 0) for i in range(n)]
        add("sliding_rectangles" if rectangular else "sliding_windows", "reduction",
            "Fixed-length overlapping ranges with moving starts.", cells, values,
            [edit(0, 0, v, [values[0] + v - 1] + values[1:]) for v in [2, 3]])

    base = [entry for i in range(n) for entry in (cell(i, 0, i + 1), cell(i, 1, 2 * (i + 1)), cell(i, 2, i % 2))]
    for changing in [False, True]:
        thresholds = [i + 1 if changing else n // 2 for i in range(n)]
        cells = base + [cell(i, 3, f'=SUMIF($A$1:$A${n},">="&{t},$B$1:$B${n})') for i, t in enumerate(thresholds)]
        values = [sum(2 * r for r in range(1, n + 1) if r >= t) for t in thresholds]
        add("criteria_changing" if changing else "criteria_repeated", "criteria", "SUMIF with shared axes and repeated or unique criteria.",
            cells, values, [edit(n - 1, 1, v, [x + v - 2 * n for x in values]) for v in [2 * n + 1, 2 * n + 2]])
    cells = base + [cell(i, 3, f'=SUMIFS($B$1:$B${n},$A$1:$A${n},">="&{i + 1},$C$1:$C${n},1)') for i in range(n)]
    values = [sum(2 * r for r in range(i + 1, n + 1) if (r - 1) % 2 == 1) for i in range(n)]
    add("criteria_multiple", "criteria", "Two criteria with independent numeric axes.", cells, values,
        [edit(1, 1, v, [x + v - 4 if i < 2 else x for i, x in enumerate(values)]) for v in [5, 6]])
    cells = [cell(i, 0, '="red"' if i % 2 == 0 else '="blue"') for i in range(n)]
    cells += [cell(i, 3, f'=COUNTIF($A$1:$A${n},"r*")+$G$1') for i in range(n)] + [cell(0, 6, 0)]
    count = (n + 1) // 2
    add("criteria_text_wildcard", "criteria", "Wildcard text criteria, with an ordinary numeric dependent input.",
        cells, [count] * n, [edit(0, 6, v, [count + v] * n) for v in [1, 2]])

    for kind in ["early", "late", "missing", "duplicates", "approximate", "text_wildcard", "unique_axes"]:
        cells = [cell(0, 6, 0)]
        values = []
        for i in range(n):
            if kind == "unique_axes":
                r = i * 4
                cells += [cell(r + j, 0, j + 1) for j in range(4)]
                cells += [cell(r + j, 1, 3 * (j + 1)) for j in range(4)]
                formula = f"=VLOOKUP(4,A{r + 1}:B{r + 4},2,0)+$G$1"
                expected = 12
            else:
                key = (i // 2 + 1) if kind == "duplicates" else i + 1
                key_input = f'="key{key:05d}"' if kind == "text_wildcard" else key * 2 if kind == "approximate" else key
                cells += [cell(i, 0, key_input), cell(i, 1, 3 * (i + 1))]
                query = 1 if kind in ["early", "duplicates"] else n if kind == "late" else n + 1
                if kind == "text_wildcard":
                    query = '"key0000?"'
                    expected = 3
                elif kind == "approximate":
                    query = 2 * (i + 1) + 1
                    expected = 3 * (i + 1)
                else:
                    expected = -1 if kind == "missing" else 3 if kind in ["early", "duplicates"] else 3 * n
                expression = f"VLOOKUP({query},$A$1:$B${n},2,{1 if kind == 'approximate' else 0})"
                # This missing-key workflow has a separate positive VLOOKUP case;
                # unsupported VLOOKUP must never be inferred from this sentinel.
                formula = f"=IFERROR({expression},-1)+$G$1" if kind == "missing" else f"={expression}+$G$1"
            cells.append(cell(i, 3, formula))
            values.append(expected)
        add("lookup_" + kind, "lookup", "Lookup position, key semantics, or axis sharing control; text keys are formula results.", cells, values,
            [edit(0, 6, v, [x + v for x in values]) for v in [1, 2]],
            capabilities=["exact_lookup"] if kind == "missing" else [])
    cells = [entry for i in range(n) for entry in (cell(i, 0, {"text": f"key{i + 1:05d}"}), cell(i, 1, 3 * (i + 1)))]
    cells += [cell(0, 6, 0)] + [cell(i, 3, f'=VLOOKUP("key{n:05d}",$A$1:$B${n},2,0)+$G$1') for i in range(n)]
    add("lookup_literal_text", "lookup", "Exact lookup over stored literal text, with a late match.", cells, [3 * n] * n,
        [edit(0, 6, v, [3 * n + v] * n) for v in [1, 2]])
    cells = [cell(i, 0, "=1/0" if i % 3 == 0 else i + 1) for i in range(n) if i % 3 != 1]
    cells += [cell(i, 3, f"=IFERROR(A{i + 1}*2,0)") for i in range(n)]
    values = [2 * (i + 1) if i % 3 == 2 else 0 for i in range(n)]
    add("errors_blanks", "coercion", "Real division errors, absent cells and numbers through ordinary arithmetic/IFERROR.", cells, values,
        [edit(0, 0, 2, [4] + values[1:]), edit(1, 0, 3, [4, 6] + values[2:])])
    # Three return columns share the selection, but edited return values stay live.
    cells = [entry for i in range(n) for entry in (cell(i, 0, i + 1), cell(i, 1, i + 1), cell(i, 2, 10 * (i + 1)))]
    cells += [cell(i, 3, f"=VLOOKUP({i + 1},$A$1:$C${n},2,0)+VLOOKUP({i + 1},$A$1:$C${n},3,0)") for i in range(n)]
    values = [11 * (i + 1) for i in range(n)]
    add("lookup_live_returns", "lookup", "Shared selection with distinct live return columns.", cells, values,
        [edit(0, 2, v, [v + 1] + values[1:]) for v in [11, 12]])

    cells = [cell(0, 6, 1)] + [cell(i, 3, f'=LEN(SUBSTITUTE("alpha-beta-{i}","-",""))+FIND("beta","alpha-beta")+$G$1') for i in range(n)]
    values = [len(f"alphabeta{i}") + 8 for i in range(n)]
    add("text_transform_search", "text", "Text transformation and position search with numeric observables.", cells, values,
        [edit(0, 6, v, [x + v - 1 for x in values]) for v in [2, 3]])
    cells = [cell(0, 6, 1)] + [cell(i, 3, f'=LEN("part"&{i}&"tail")+$G$1') for i in range(n)]
    values = [8 + len(str(i)) + 1 for i in range(n)]
    add("text_concatenation", "text", "Distinct literal fragments and numeric-to-text conversion.", cells, values,
        [edit(0, 6, v, [x + v - 1 for x in values]) for v in [2, 3]])
    cells = [cell(0, 6, 1)] + [cell(i, 3, f"=DATE(2024,1,{i + 1})-DATE(2024,1,1)+$G$1") for i in range(n)]
    add("date_arithmetic", "date", "Gregorian date construction across month/year boundaries.", cells, [i + 1 for i in range(n)],
        [edit(0, 6, v, [i + v for i in range(n)]) for v in [2, 3]])
    start = datetime.date(2024, 1, 1)
    days = [sum((start + datetime.timedelta(days=j)).weekday() < 5 for j in range(i + 1)) for i in range(n)]
    cells = [cell(0, 6, 0)] + [cell(i, 3, f"=NETWORKDAYS(DATE(2024,1,1),DATE(2024,1,{i + 1}))+$G$1") for i in range(n)]
    add("business_days", "date", "Business-day calendars with independently counted weekdays.", cells, days,
        [edit(0, 6, v, [x + v for x in days]) for v in [1, 2]])
    for operation in ["MEDIAN", "VAR.P", "VARP"]:
        cells = [cell(i, 0, i + 1) for i in range(n)] + [cell(0, 6, 0)]
        cells += [cell(i, 3, f"={operation}($A$1:$A${n})+$G$1") for i in range(n)]
        mean = (n + 1) / 2
        answer = mean if operation == "MEDIAN" else sum((i + 1 - mean) ** 2 for i in range(n)) / n
        add("statistics_" + operation.lower().replace(".", "_"), "statistics", "Order statistics versus a population-variance reduction.", cells, [answer] * n,
            [edit(0, 6, v, [answer + v] * n) for v in [1, 2]],
            capabilities=["modern_variance"] if operation == "VAR.P" else [])
    cash = [i + 1 for i in range(n)]
    answer = sum(value / 1.05 ** (i + 1) for i, value in enumerate(cash))
    cells = [cell(i, 0, value) for i, value in enumerate(cash)] + [cell(0, 6, 0)]
    cells += [cell(i, 3, f"=NPV(0.05,$A$1:$A${n})+$G$1") for i in range(n)]
    add("financial_npv", "finance", "Discounted cash-flow reductions with repeated ranges.", cells, [answer] * n,
        [edit(0, 6, v, [answer + v] * n) for v in [1, 2]])
    cells = [entry for i in range(n) for entry in (cell(i, 0, i + 1), cell(i, 1, 2))] + [cell(0, 6, 0)]
    cells += [cell(i, 3, f"=SUMPRODUCT($A$1:$A${n},$B$1:$B${n})+$G$1") for i in range(n)]
    answer = n * (n + 1)
    add("array_sumproduct", "array", "Pairwise range multiplication and reduction.", cells, [answer] * n,
        [edit(0, 6, v, [answer + v] * n) for v in [1, 2]])
    cells = [cell(0, 6, 0)] + [cell(i, 3, "=SUM(MMULT({1,2;3,4},{2;1}))+$G$1") for i in range(n)]
    add("matrix_multiply", "array", "Small matrix products, materialization and scalar reduction.", cells, [14] * n,
        [edit(0, 6, v, [14 + v] * n) for v in [1, 2]])

    for dynamic in [False, True]:
        rows = max(2, n // 4)
        cells = [cell(0, 6, rows), cell(0, 0, "=SEQUENCE($G$1,2)" if dynamic else f"=SEQUENCE({rows},2)+$G$1")]
        out = [address(r, c) for r in range(rows) for c in range(2)]
        values = [i + 1 + (0 if dynamic else rows) for i in range(2 * rows)]
        # An observable beyond the initial extent detects both growth and stale
        # children after shrink, rather than only unchanged old SEQUENCE entries.
        if dynamic:
            cells.append(cell(0, 3, f"=SUM(A{rows + 1}:B{rows + 1})"))
            out.append(address(0, 3))
            values.append(0)
        changes = [rows + 1, rows] if dynamic else [rows + 1, rows + 2]
        edits = [edit(0, 6, v, list(range(1, 2 * rows + 1)) + [4 * rows + 3 if v > rows else 0]
                      if dynamic else [i + 1 + v for i in range(2 * rows)]) for v in changes]
        add("spill_dynamic" if dynamic else "spill_fixed", "spill", "A rectangular producer, including its non-anchor children.", cells, values,
            edits, out=out, capabilities=["dynamic_arrays", "dynamic_array_dimensions"] if dynamic else ["dynamic_arrays"])
    for many in [False, True]:
        cells = [cell(0, 6, 0)] + [cell(i * 3, 0, "=SEQUENCE(2,2)+$G$1") for i in range(n)]
        out = [address(i * 3 + 1, 1) for i in range(n)] if many else [address(1, 1)]
        add("spill_many_all" if many else "spill_many_one", "spill", "Many disjoint producers with one versus every child requested.", cells, [4] * len(out),
            [edit(0, 6, v, [4 + v] * len(out)) for v in [1, 2]], out=out, capabilities=["dynamic_arrays"])
    cells = [cell(0, 6, 0)] + [cell(i * 3, 0, "=SEQUENCE(1,IF($G$1=0,2,1))") for i in range(n)]
    add("spill_unknown_width", "spill", "Conservative unknown width; disjoint actual footprints.", cells, [1] * n,
        [edit(0, 6, v, [1] * n) for v in [1, 0]], out=[address(i * 3, 0) for i in range(n)], capabilities=["dynamic_arrays", "dynamic_array_dimensions"])
    # Both possible rectangles can intersect but actual rectangles remain disjoint.
    cells = [cell(0, 6, 0)]
    for i in range(n):
        cells += [cell(i * 4, 0, "=SEQUENCE(IF($G$1=0,1,2),2)"), cell(i * 4 + 2, 1, "=SEQUENCE(1,2)")]
    add("spill_competing_bounds", "spill", "Overlapping possible bounds with disjoint actual spills.", cells, [2] * n,
        [edit(0, 6, v, [2] * n) for v in [1, 0]], out=[address(i * 4 + 2, 2) for i in range(n)], capabilities=["dynamic_arrays", "dynamic_array_dimensions"])
    cells = [dict(cell(i, 0, i + 1), sheet=1) for i in range(n)]
    cells += [cell(i, 3, f"=Sheet2!A{i + 1}*2") for i in range(n)]
    values = [2 * (i + 1) for i in range(n)]
    add("cross_sheet", "source", "Cross-sheet dependencies with a single changed source.", cells, values,
        [dict(edit(0, 0, v, [2 * v] + values[1:]), sheet=1) for v in [2, 3]], sheet_count=2)
    cells = [cell(i, 0, i + 1) for i in range(n)] + [cell(i, 3, f"=A{i + 1}*2") for i in range(n)]
    add("formula_source_edit", "source", "Change source formula structure while retaining other branches.", cells, [2 * (i + 1) for i in range(n)],
        [edit(0, 3, "=A1*3", [3] + [2 * (i + 1) for i in range(1, n)]),
         edit(0, 3, "=A1+5", [6] + [2 * (i + 1) for i in range(1, n)])])
    return suite


def main():
    """Emit a complete suite for direct adapter use or inspection."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--size", type=int, default=64)
    parser.add_argument("--samples", type=int, default=3)
    parser.add_argument("--warmup", type=int, default=1)
    args = parser.parse_args()
    print(json.dumps(generate_coverage(args.size, args.samples, args.warmup), allow_nan=False))


if __name__ == "__main__":
    main()
