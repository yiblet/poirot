# SERIESSUM

`SERIESSUM(x, n, m, coefficients)` sums terms of the form
`coefficient[i] * x^(n + i*m)` in row-major coefficient order.
Both `PinnedPoi` and `ExcelDocumentation` select this contract.

## Accepted arguments

Scalar `x`, `n`, `m` and scalar coefficients, including single-cell references,
accept numbers, blanks as zero, and contextual numeric text. Logicals, empty text
and omitted arguments produce `#VALUE!`.

Coefficient arrays, including 1×1 arrays, and multi-cell references accept numbers
and blanks as zero. Text and logical members produce `#VALUE!`. Every blank keeps
its exponent position. Multi-sheet references traverse sheet-first, then row-major.

## Calculation and errors

Each coefficient is converted immediately before calculating its term. Earlier
argument errors take precedence; an invalid earlier term takes precedence over
a later coefficient error. Exponents are calculated directly for each term.

Zero to zero is one. Zero to a negative exponent, nonreal powers, nonfinite terms
and overflowing intermediate sums produce `#NUM!`, even for a zero coefficient.
Later cancellation cannot repair an earlier overflow. These rules are specific
to SERIESSUM and differ from the scalar POWER error contract.

Traversal and numeric-text parsing use normal evaluation budgets. Operational
failures escape `IFERROR`.

The series definition follows Microsoft's documented function. Detailed argument
and term behavior follows the pinned ClosedXML implementation and tests; complete
native Excel boundary compatibility is not established.
