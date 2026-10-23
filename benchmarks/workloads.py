"""Original deterministic calculation workloads; no spreadsheet file I/O or fixtures."""


def cell(row, col, value):
    """Describe one zero-based input or formula for the shared protocol."""
    return {"row": row, "col": col, "input": value}


def address(row, col):
    """Describe a zero-based output coordinate."""
    return {"row": row, "col": col}


def case(name, description, cells, outputs, expected, edits):
    """Attach independently derived expected results to a workload."""
    return dict(name=name, description=description, cells=cells, outputs=outputs,
                expected=expected, edits=edits)


def edit(row, col, value, expected):
    """Describe one changed input and the resulting complete output vector."""
    return dict(row=row, col=col, value=value, expected=expected)


def generate(size=512, samples=15, warmup=3):
    """Generate identical arithmetic, aggregation, branching, lookup, and chain models."""
    if not 4 <= size <= 5000 or not 1 <= samples <= 1000 or not 0 <= warmup <= 100:
        raise ValueError("size must be 4..5000, samples 1..1000, warmup 0..100")
    n = size
    series = n * (n + 1) // 2
    cases = []

    cells = [entry for r in range(1, n + 1)
             for entry in (cell(r - 1, 0, r), cell(r - 1, 1, f"=A{r}*1.25+3"))]
    cells.append(cell(0, 2, f"=SUM(B1:B{n})"))
    total = series * 1.25 + 3 * n
    cases.append(case("arithmetic", "Independent arithmetic rows with a SUM output.",
                      cells, [address(0, 2)], [total],
                      [edit(0, 0, 2, [total + 1.25]), edit(0, 0, 3, [total + 2.5])]))

    cells = [cell(0, 0, 1)] + [cell(r - 1, 1, f"=$A$1*{r}") for r in range(1, n + 1)]
    cells.append(cell(0, 2, f"=SUM(B1:B{n})"))
    cases.append(case("fan_out", "One input feeding every arithmetic branch.",
                      cells, [address(0, 2)], [series],
                      [edit(0, 0, 2, [2 * series]), edit(0, 0, 3, [3 * series])]))

    cells = [entry for r in range(1, n + 1)
             for entry in (cell(r - 1, 0, r), cell(r - 1, 1, f"=SUM($A$1:A{r})"))]
    expected = [r * (r + 1) // 2 for r in range(1, n + 1)]
    outputs = [address(r, 1) for r in range(n)]
    cases.append(case("prefix_sums", "Growing overlapping ranges; read every formula.",
                      cells, outputs, expected,
                      [edit(0, 0, 2, [v + 1 for v in expected]),
                       edit(0, 0, 3, [v + 2 for v in expected])]))

    half = n // 2
    cells = [entry for r in range(1, n + 1)
             for entry in (cell(r - 1, 0, r),
                           cell(r - 1, 1, f"=IF(A{r}>{half},A{r}*2,A{r}+1)"))]
    cells.append(cell(0, 2, f"=SUM(B1:B{n})"))
    low_sum = half * (half + 1) // 2
    total = low_sum + half + 2 * (series - low_sum)
    cases.append(case("conditional", "IF branches selected from numeric inputs.",
                      cells, [address(0, 2)], [total],
                      [edit(0, 0, 0, [total - 1]), edit(0, 0, 2, [total + 1])]))

    cells = [entry for r in range(1, n + 1)
             for entry in (cell(r - 1, 0, r), cell(r - 1, 1, r * 3),
                           cell(r - 1, 3, f"=VLOOKUP({r},$A$1:$B${n},2,0)"))]
    cells.append(cell(0, 4, f"=SUM(D1:D{n})"))
    cases.append(case("exact_lookup", "Exact VLOOKUP over a shared numeric table.",
                      cells, [address(0, 4)], [series * 3],
                      [edit(0, 1, 4, [series * 3 + 1]), edit(0, 1, 5, [series * 3 + 2])]))

    # Bound nesting below default depth limits; breadth workloads scale with size.
    depth = min(n, 32)
    cells = [cell(0, 0, 1), cell(0, 1, "=A1+1")]
    cells += [cell(r - 1, 1, f"=B{r - 1}+1") for r in range(2, depth + 1)]
    cases.append(case("dependency_chain", f"A {depth}-formula dependent chain.",
                      cells, [address(depth - 1, 1)], [depth + 1],
                      [edit(0, 0, 2, [depth + 2]), edit(0, 0, 3, [depth + 3])]))

    cells = [entry for r in range(1, n + 1)
             for entry in (cell(r - 1, 0, r), cell(r - 1, 1, f"=A{r}*2"))]
    expected = [r * 2 for r in range(1, n + 1)]
    cases.append(case("independent_branches", "Edit one branch while reading every output.",
                      cells, outputs, expected,
                      [edit(0, 0, 2, [4] + expected[1:]),
                       edit(0, 0, 3, [6] + expected[1:])]))
    return dict(schema_version=1, warmup=warmup, samples=samples, cases=cases)
