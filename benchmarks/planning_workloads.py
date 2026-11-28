"""Deterministic planning scenarios, including edits to facts and shared assumptions."""

from statistics import mean
from workloads import address, case, cell, edit


def charges(size):
    """Build distinct transaction-level charging, compensation and recovery rules."""
    specifications = [
        ("overtime_pay", "Calculate base and overtime wages for each shift.",
         "=MIN(A{r},$F$1)*B{r}+MAX(0,A{r}-$F$1)*B{r}*1.5",
         lambda hours, rate, limit: min(hours, limit) * rate + max(0, hours - limit) * rate * 1.5),
        ("project_retention", "Withhold a percentage until each invoice clears its release threshold.",
         "=IF(A{r}<$F$1,B{r}*0.9,B{r})",
         lambda progress, invoice, limit: invoice * 0.9 if progress < limit else invoice),
        ("shipping_surcharge", "Apply a surcharge to orders above a shared weight threshold.",
         "=B{r}+ROUNDUP(A{r}/$F$1,0)*2",
         lambda weight, base, limit: base + ((weight + limit - 1) // limit) * 2),
        ("insurance_deductible", "Apply a deductible and a reimbursement cap to each covered loss.",
         "=MIN(MAX(A{r}-$F$1,0),B{r})",
         lambda loss, cap, deductible: min(max(loss - deductible, 0), cap)),
    ]
    for name, description, formula, oracle in specifications:
        quantities = [5 + (i * 7) % 48 for i in range(size)]
        rates = [12 + (i * 3) % 27 for i in range(size)]
        limit = 22
        cells = [cell(0, 5, limit)]
        for i, (quantity, rate) in enumerate(zip(quantities, rates)):
            cells.extend([cell(i, 0, quantity), cell(i, 1, rate),
                          cell(i, 2, formula.format(r=i + 1))])
        cells.append(cell(0, 3, f"=SUM(C1:C{size})"))

        def results(shared):
            """Compute each amount and its ledger total from the original numbers."""
            amounts = [oracle(a, b, shared) for a, b in zip(quantities, rates)]
            return [sum(amounts), *amounts]

        expected = results(limit)
        quantities[0] += 31
        edits = [edit(0, 0, quantities[0], results(limit)),
                 edit(0, 5, 17, results(17))]
        yield dict(case(name, description, cells,
                        [address(0, 3), *(address(i, 2) for i in range(size))],
                        expected, edits), family="planning")


def rolling_decisions(size):
    """Read rolling observations at different addresses from their source amounts."""
    specifications = [
        ("demand_smoothing", "Smooth demand across the latest six periods.",
         "AVERAGE({range})*$F$1", lambda values, factor: mean(values) * factor),
        ("peak_load_reserve", "Reserve capacity for the peak of the latest six periods.",
         "MAX({range})+$F$1", lambda values, reserve: max(values) + reserve),
        ("quality_exception_rate", "Count measured exceptions above a shared limit.",
         'COUNTIF({range},">"&$F$1)/{length}',
         lambda values, limit: sum(value > limit for value in values) / len(values)),
    ]
    for name, description, formula, oracle in specifications:
        history = [2 + (i * 11) % 37 for i in range(size)]
        windows = [(max(0, i - 5), i + 1) for i in range(size)]
        cells = [cell(0, 5, 10), *(cell(i, 0, value) for i, value in enumerate(history))]
        for i, (first, last) in enumerate(windows):
            expression = formula.format(range=f"A{first + 1}:A{last}", length=last - first)
            cells.append(cell(i, 2, "=" + expression))

        def results(shared):
            """Apply the decision rule to each explicit window."""
            return [oracle(history[first:last], shared) for first, last in windows]

        expected = results(10)
        changed = size // 2
        history[changed] += 24
        edits = [edit(changed, 0, history[changed], results(10)), edit(0, 5, 15, results(15))]
        yield dict(case(name, description, cells, [address(i, 2) for i in range(size)],
                        expected, edits), family="planning")


def weighted_reports(size):
    """Compare operational quantities by using aligned data from two worksheets."""
    volume = [1 + (i * 3) % 19 for i in range(size)]
    rates = [4 + i % 11 for i in range(size)]
    specifications = [
        ("energy_cost", "Calculate energy consumption at meter-specific tariffs.",
         lambda n: f"SUMPRODUCT(Sheet2!A1:A{n},Sheet2!B1:B{n})+$C$1",
         lambda v, r, shared: sum(a * b for a, b in zip(v, r)) + shared),
        ("weighted_quality_score", "Weight site scores by observations and add a calibration adjustment.",
         lambda n: f"SUMPRODUCT(Sheet2!A1:A{n},Sheet2!B1:B{n})/SUM(Sheet2!A1:A{n})+$C$1",
         lambda v, r, shared: sum(a * b for a, b in zip(v, r)) / sum(v) + shared),
    ]
    for name, description, formula, oracle in specifications:
        cells = [cell(0, 0, "=" + formula(size)), cell(0, 2, 2)]
        for i, (amount, rate) in enumerate(zip(volume, rates)):
            cells.extend([dict(cell(i, 0, amount), sheet=1), dict(cell(i, 1, rate), sheet=1)])
        expected = [oracle(volume, rates, 2)]
        updated = [volume[0] + 9, *volume[1:]]
        edits = [dict(edit(0, 0, updated[0], [oracle(updated, rates, 2)]), sheet=1),
                 edit(0, 2, 4, [oracle(updated, rates, 4)])]
        yield dict(case(name, description, cells, [address(0, 0)], expected, edits),
                   family="planning", sheet_count=2)
