"""Original workbook scenarios for operations and financial planning."""

from workloads import address, case, cell, edit


def inventory_replenishment(size):
    """Carry stock forward with reorder points and a safety-stock target."""
    cells = [cell(0, 6, 120), cell(1, 6, 45), cell(2, 6, 100)]
    demand = [11 + (i * 7) % 31 for i in range(size)]
    for i, units in enumerate(demand):
        row = i + 1
        prior = "$G$1" if i % 16 == 0 else f"D{i}"
        cells.extend([cell(i, 0, units), cell(i, 1, f"={prior}-A{row}"),
                      cell(i, 2, f"=IF(B{row}<$G$2,$G$3-B{row},0)"),
                      cell(i, 3, f"=B{row}+C{row}")])
    cells.append(cell(0, 4, f"=SUM(C1:C{size})"))

    def results(reorder_point):
        """Simulate daily stock and supply orders in each 16-day location schedule."""
        order_total = 0
        balances = []
        for i, units in enumerate(demand):
            current = (120 if i % 16 == 0 else balances[-1]) - units
            if current < reorder_point:
                order_total += 100 - current
                current = 100
            balances.append(current)
        return [order_total, *balances]

    outputs = [address(0, 4), *(address(i, 3) for i in range(size))]
    expected = results(45)
    demand[size // 2] += 23
    edits = [edit(size // 2, 0, demand[size // 2], results(45)),
             edit(1, 6, 75, results(75))]
    return dict(case("inventory_replenishment", "Update consumption and a shared reorder point.",
                     cells, outputs, expected, edits), family="operations")


def capacity_quotas(size):
    """Calculate realized production against station bottlenecks and shift targets."""
    # Capacities differ by station and shift so the limiting station can move.
    quotas = [52 + i % 10 for i in range(size)]
    cells = []
    for i, quota in enumerate(quotas):
        row = i + 1
        raw = (48 + (i * 3) % 27, 50 + (i * 7) % 19, 46 + (i * 11) % 35)
        cells.extend(dict(cell(i, col, value), sheet=1)
                     for col, value in enumerate([*raw, quota]))
        cells.extend([cell(i, 0, f"=MIN(Sheet2!A{row}:D{row})"),
                      cell(i, 1, f"=MAX(0,Sheet2!D{row}-A{row})")])
    cells.extend([cell(0, 3, f"=SUM(A1:A{size})"), cell(1, 3, f"=SUM(B1:B{size})"),
                  cell(2, 3, f"=MAX(B1:B{size})")])

    def results():
        """Calculate production and backlog from physical limits and orders."""
        actual = [min(48 + (i * 3) % 27, 50 + (i * 7) % 19,
                      46 + (i * 11) % 35, quota) for i, quota in enumerate(quotas)]
        backlog = [quota - made for quota, made in zip(quotas, actual)]
        return [sum(actual), sum(backlog), max(backlog)]

    expected = results()
    edits = []
    for i, quota in [(0, 40), (size - 1, 90)]:
        quotas[i] = quota
        edits.append(dict(edit(i, 3, quota, results()), sheet=1))
    return dict(case("capacity_quotas", "Update two orders and observe throughput constraints.",
                     cells, [address(i, 3) for i in range(3)], expected, edits),
                family="operations", sheet_count=2)


def receivables_aging(size):
    """Track amounts outstanding across three aging bands and departments."""
    entries = [(i % 3 + 1, (i * 13) % 96, 125 + i * 9, 0 if i % 5 == 0 else 1)
               for i in range(size)]
    cells = []
    for i, values in enumerate(entries):
        row = i + 1
        cells.extend(cell(i, col, value) for col, value in enumerate(values))
        cells.extend([cell(i, 4, f"=IF(D{row}=1,0,C{row})"),
                      cell(i, 5, f"=IF(B{row}<30,1,IF(B{row}<60,2,3))")])
    for group in range(3):
        for band in range(3):
            cells.append(cell(group, 7 + band,
                              f"=SUMIFS(E1:E{size},A1:A{size},{group + 1},F1:F{size},{band + 1})"))

    def results():
        """Place unpaid balances in aging bands using integer day offsets."""
        totals = [0] * 9
        for group, days, amount, paid in entries:
            if not paid:
                totals[(group - 1) * 3 + min(2, days // 30)] += amount
        return totals

    expected = results()
    group, _, amount, _ = entries[0]
    entries[0] = (group, 67, amount, 0)
    edits = [edit(0, 1, 67, results())]
    entries[0] = (group, 67, amount, 1)
    edits.append(edit(0, 3, 1, results()))
    outputs = [address(group, 7 + band) for group in range(3) for band in range(3)]
    return dict(case("receivables_aging", "Age a receivable into a new band and post payment.",
                     cells, outputs, expected, edits), family="operations")
