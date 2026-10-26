# poirot-functions-lookup

Look up spreadsheet values and select references.

Use `evaluate` with a selected function and its arguments. Implement the
`Workbook` trait to supply cell reads and workbook metadata. Results retain
references when the spreadsheet function returns one.

Lower-level helpers cover `MATCH`, `XMATCH`, approximate search, `INDEX`, `ADDRESS` text, and parsing dynamic references. The caller evaluates arguments and controls limits. `address_text` formats one-based
coordinates with typed absolute-axis and A1/R1C1 notation choices.
`intersection_offsets` selects the unique one-based point of a singleton or
vector reference; the evaluator owns scalar/array selection and publication.
`offset_extent` translates one axis with checked arithmetic and modern worksheet
bounds. The evaluator prepares OFFSET controls and publishes its reference;
OFFSET is no longer part of the workbook-backed `evaluate` helper API. Negative
sizes normalize toward the displaced cell as a compatibility policy whose native
Excel behavior is unverified.

Construct ordinary references with `Reference::new(kind, workbook, sheets, rows,
cols)`, using inclusive zero-based ranges. The constructor rejects reversed and
off-grid worksheet bounds with `Failure::Spreadsheet(Error::Ref)`. Read identity
and coordinates through accessors; dimensions are always nonzero. Signed source
references remain separate to retain intentional off-grid or reversed source
geometry.

Construct rectangular arrays with `Array::new(rows, cols, values)`. Empty,
overflowing, and mismatched shapes return `Failure::Spreadsheet(Error::Value)`.
Use `rows()`, `cols()`, and `values()` to inspect an array, `values_mut()` to replace
members without changing its shape, and `into_values()` to consume its storage.
These constructors and accessors replace public struct literals and field access;
ordinary references no longer require a separate `valid()` check.
