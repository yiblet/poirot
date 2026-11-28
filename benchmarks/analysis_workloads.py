"""Original decision, reconciliation and sequential calculation models."""

from itertools import accumulate
from statistics import median

from workloads import address, case, cell, edit


def decisions(size):
    """Read every classification after a row edit and a change to shared policy."""
    rules = [
        ("shift_coverage", '=IF(AND(A{r}>=F$1,B{r}>=12),1,0)',
         lambda a, b, limit: int(a >= limit and b >= 12)),
        ("inspection_escalation", '=IF(OR(A{r}>F$1,B{r}<8),2,1)',
         lambda a, b, limit: 2 if a > limit or b < 8 else 1),
        ("settlement_tolerance", '=IF(ABS(A{r}-B{r})<=F$1,0,A{r}-B{r})',
         lambda a, b, limit: 0 if abs(a - b) <= limit else a - b),
        ("reservation_lots", '=INT(A{r}/F$1)*B{r}',
         lambda a, b, limit: a // limit * b),
        ("maintenance_cycle", '=IF(MOD(A{r},F$1)=0,B{r},0)',
         lambda a, b, limit: b if a % limit == 0 else 0),
        ("rebate_brackets", '=IF(A{r}<F$1,0,IF(A{r}<2*F$1,B{r}/4,B{r}/2))',
         lambda a, b, limit: 0 if a < limit else b / (4 if a < 2 * limit else 2)),
    ]
    for name, formula, oracle in rules:
        first = [4 + (i * 7) % 35 for i in range(size)]
        second = [5 + (i * 3) % 17 for i in range(size)]
        cells = [cell(0, 5, 13)]
        for i, (a, b) in enumerate(zip(first, second)):
            cells.extend([cell(i, 0, a), cell(i, 1, b), cell(i, 2, formula.format(r=i + 1))])

        def results(limit):
            """Apply the row rule without evaluating any spreadsheet formula."""
            return [oracle(a, b, limit) for a, b in zip(first, second)]

        initial = results(13)
        first[0] = 31
        yield dict(case(name, name.replace("_", " ").capitalize() + " for each observation.",
                        cells, [address(i, 2) for i in range(size)], initial,
                        [edit(0, 0, 31, results(13)), edit(0, 5, 9, results(9))]),
                   family="decisions")


def reconciliations(size):
    """Combine per-group range reductions through a small reporting layer."""
    reports = [
        ("shipment_control_total", "SUM(A{start}:A{end})-SUM(B{start}:B{end})",
         lambda a, b: sum(a) - sum(b)),
        ("checkout_average_ticket", "SUM(A{start}:A{end})/SUM(B{start}:B{end})",
         lambda a, b: sum(a) / sum(b)),
        ("sensor_centering", "MEDIAN(A{start}:A{end})-AVERAGE(B{start}:B{end})",
         lambda a, b: median(a) - sum(b) / len(b)),
        ("stocktake_spread", "MAX(A{start}:A{end})-MIN(B{start}:B{end})",
         lambda a, b: max(a) - min(b)),
        ("service_target_attainment", 'COUNTIF(A{start}:A{end},">=20")/COUNT(B{start}:B{end})',
         lambda a, b: sum(n >= 20 for n in a) / len(b)),
        ("wallet_net_exposure", "ABS(SUM(A{start}:A{end})-SUM(B{start}:B{end}))",
         lambda a, b: abs(sum(a) - sum(b))),
    ]
    for name, formula, oracle in reports:
        first = [12 + (i * 7) % 23 for i in range(size)]
        second = [4 + (i * 3) % 27 for i in range(size)]
        groups = [(i, min(i + 5, size)) for i in range(0, size, 5)]
        cells = [entry for i, (a, b) in enumerate(zip(first, second))
                 for entry in (cell(i, 0, a), cell(i, 1, b))]
        for i, (start, end) in enumerate(groups):
            cells.append(cell(i, 2, "=" + formula.format(start=start + 1, end=end) + "+$F$1"))
        cells.extend([cell(0, 5, 0), cell(0, 3, f"=SUM(C1:C{len(groups)})")])
        outputs = [address(0, 3), *(address(i, 2) for i in range(len(groups)))]

        def results(adjustment):
            """Summarize fixed groups and reconcile their total."""
            values = [oracle(first[start:end], second[start:end]) + adjustment
                      for start, end in groups]
            return [sum(values), *values]

        initial = results(0)
        first[-1] = 38
        yield dict(case(name, name.replace("_", " ").capitalize() + " across five-row groups.",
                        cells, outputs, initial,
                        [edit(size - 1, 0, 38, results(0)), edit(0, 5, 2, results(2))]),
                   family="reconciliation")


def trajectories(size):
    """Evaluate recurrence segments, including edits in the middle of one segment."""
    paths = [
        ("storage_balance", "MAX(0,{previous}+A{row}-D$1)",
         lambda previous, value, limit: max(0, previous + value - limit)),
        ("cooldown_budget", "MIN(D$1,{previous}+A{row})",
         lambda previous, value, limit: min(limit, previous + value)),
        ("equipment_depreciation", "{previous}*(1-D$1/100)+A{row}",
         lambda previous, value, rate: previous * (1 - rate / 100) + value),
        ("portfolio_high_water", "MAX({previous},A{row}-D$1)",
         lambda previous, value, offset: max(previous, value - offset)),
        ("backlog_clearance", "IF(A{row}>D$1,{previous}+A{row}-D$1,0)",
         lambda previous, value, capacity: previous + value - capacity if value > capacity else 0),
        ("shock_adjustment", "IF(A{row}<D$1,{previous}/2,{previous}+A{row})",
         lambda previous, value, threshold: previous / 2 if value < threshold else previous + value),
    ]
    for name, formula, advance in paths:
        values = [5 + (i * 7) % 29 for i in range(size)]
        segments = [values[i:i + 12] for i in range(0, size, 12)]
        cells = [cell(0, 3, 10)]
        for i, value in enumerate(values):
            prior = "0" if i % 12 == 0 else f"B{i}"
            cells.extend([cell(i, 0, value), cell(i, 1, "=" + formula.format(previous=prior, row=i + 1))])

        def results(setting):
            """Restart each segment at zero and track every intermediate value."""
            return [answer for part in segments
                    for answer in list(accumulate(part, lambda prior, value: advance(prior, value, setting),
                                                  initial=0))[1:]]

        initial = results(10)
        middle = size // 2
        segment, offset = divmod(middle, 12)
        segments[segment][offset] = 38
        yield dict(case(name, name.replace("_", " ").capitalize() + " with independent reset periods.",
                        cells, [address(i, 1) for i in range(size)], initial,
                        [edit(middle, 0, 38, results(10)), edit(0, 3, 16, results(16))]),
                   family="trajectories")
