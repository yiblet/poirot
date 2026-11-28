"""Synthetic business workbooks with numeric oracles independent of the adapters."""

from workloads import address, case, cell, edit


def order_register(size):
    """Price order lines from a catalog and roll up posted revenue by territory."""
    prices = [12, 19, 25, 40, 55, 80, 110]
    lines = [(i % len(prices), 1 + i % 9, (i // 3) % 4, i % 11 != 0)
             for i in range(size)]
    cells = []
    for index, price in enumerate(prices):
        cells.extend([dict(cell(index, 0, index + 1), sheet=2),
                      dict(cell(index, 1, price), sheet=2)])
    for i, (item, quantity, territory, posted) in enumerate(lines):
        r = i + 1
        row = [item + 1, quantity, territory + 1, int(posted),
               f"=IF(D{r}=1,B{r}*VLOOKUP(A{r},Sheet3!$A$1:$B$7,2,0),0)"]
        cells.extend(dict(cell(i, col, value), sheet=1)
                     for col, value in enumerate(row))
    for territory in range(4):
        cells.append(cell(territory, 0, f"=SUMIF(Sheet2!C1:C{size},{territory + 1},Sheet2!E1:E{size})"))
    cells.append(cell(4, 0, "=SUM(A1:A4)"))

    def totals():
        """Sum posted invoice amounts using the current source records."""
        revenue = [0] * 4
        for item, quantity, territory, posted in lines:
            if posted:
                revenue[territory] += quantity * prices[item]
        return revenue + [sum(revenue)]

    expected = totals()
    item, quantity, territory, _ = lines[0]
    lines[0] = (item, quantity, territory, True)
    edits = [dict(edit(0, 3, 1, totals()), sheet=1)]
    prices[0] += 5
    edits.append(dict(edit(0, 1, prices[0], totals()), sheet=2))
    return dict(case("order_register", "Post an order, then update a shared catalog price.",
                     cells, [address(i, 0) for i in range(5)], expected, edits),
                sheet_count=3, family="business")


def cash_plan(size):
    """Roll forward monthly cash from recurring invoices and operating costs."""
    accounts = [(75 + i * 3, 1 + i % 6, 7 + i % 6) for i in range(size)]
    opening_cash = 10_000
    monthly_cost = 2_500
    cells = [cell(0, 3, opening_cash), cell(1, 3, monthly_cost)]
    for i, (invoice, start, end) in enumerate(accounts):
        r = i + 1
        cells.extend(dict(cell(i, col, value), sheet=1)
                     for col, value in enumerate([invoice, start, end]))
        cells.extend(dict(cell(i, month + 2,
                               f"=IF(AND({month}>=B{r},{month}<=C{r}),A{r},0)"), sheet=1)
                     for month in range(1, 13))
    for i, col in enumerate("DEFGHIJKLMNO"):
        r = i + 1
        prior = "$D$1" if i == 0 else f"B{i}"
        cells.extend([cell(i, 0, f"=SUM(Sheet2!{col}1:{col}{size})"),
                      cell(i, 1, f"={prior}+A{r}-$D$2")])

    def balances():
        """Accumulate cash independently from invoice start and end periods."""
        cash = opening_cash
        closing = []
        for month in range(1, 13):
            cash += sum(invoice for invoice, start, end in accounts if start <= month <= end)
            cash -= monthly_cost
            closing.append(cash)
        return closing

    expected = balances()
    invoice, start, end = accounts[-1]
    accounts[-1] = (invoice, start, end - 1)
    edits = [dict(edit(size - 1, 2, end - 1, balances()), sheet=1)]
    monthly_cost += 100
    edits.append(edit(1, 3, monthly_cost, balances()))
    return dict(case("cash_plan", "Shorten an invoice schedule, then raise recurring costs.",
                     cells, [address(i, 1) for i in range(12)], expected, edits),
                sheet_count=2, family="business")


def bank_reconciliation(size):
    """Match ledger entries to imported bank records and flag unexplained amounts."""
    ledger = {f"REF-{i + 1:05d}": (-1 if i % 4 == 3 else 1) * (60 + 5 * i)
              for i in range(size)}
    statement = {}
    cells = []
    for i, (reference, amount) in enumerate(ledger.items()):
        r = i + 1
        bank_ref = f"PENDING-{r:05d}" if i % 5 == 0 else reference
        bank_amount = amount + (3 if i % 7 == 1 else 0)
        statement[bank_ref] = bank_amount
        cells.extend([cell(i, 0, {"text": reference}), cell(i, 1, amount),
                      cell(i, 2, f"=B{r}-IFERROR(VLOOKUP(A{r},Sheet2!$A$1:$B${size},2,0),0)"),
                      cell(i, 3, f"=ABS(C{r})"), cell(i, 4, f"=IF(C{r}=0,0,1)"),
                      dict(cell(i, 0, {"text": bank_ref}), sheet=1),
                      dict(cell(i, 1, bank_amount), sheet=1)])
    cells.extend(cell(0, 5 + i, f"=SUM({col}1:{col}{size})")
                 for i, col in enumerate("CDE"))

    def totals():
        """Count unmatched or different amounts by joining the raw sources."""
        gaps = [amount - statement.get(reference, 0) for reference, amount in ledger.items()]
        return [sum(gaps), sum(map(abs, gaps)), sum(gap != 0 for gap in gaps)]

    expected = totals()
    statement["REF-00001"] = statement.pop("PENDING-00001")
    edits = [dict(edit(0, 0, {"text": "REF-00001"}, totals()), sheet=1)]
    statement["REF-00002"] = ledger["REF-00002"]
    edits.append(dict(edit(1, 1, ledger["REF-00002"], totals()), sheet=1))
    return dict(case("bank_reconciliation",
                     "Resolve an unmatched transaction ID, then correct an amount.",
                     cells, [address(0, i) for i in range(5, 8)], expected, edits),
                sheet_count=2, family="business")
