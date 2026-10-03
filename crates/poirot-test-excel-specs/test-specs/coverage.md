# Excel E2E coverage inventory

The single table contains 2,003 cases, 2,059 evaluation steps and 2,060 assertions citing 261 Microsoft sources. 256 of 256 registered function contracts have source-backed specifications. This is specification breadth, not verified passing coverage.

Of 1,622 potential obligations, 563 have documented assertion examples, 1051 have no case, 0 have unresolved expectations, and 8 are not applicable. Generic facets marked requires_research are not considered exhaustive merely because examples exist.

Workbook coverage includes 45 multistep cases, 17 named fixtures, 6 fixtures in the 1904 date system, and 16 complete worksheet-footprint assertions.

Evidence: 2,013 documented derivation, 47 documented example. No ROI evaluator tests or Excel observations were run during authoring.

The complete machine-readable missing-obligation and capability lists are in [coverage.json](coverage.json), with scope and individual obligations in [inventory.json](inventory.json). Passing behavior remains unverified.

## Scoped execution capabilities

Registered means membership in the default 256-entry registry. Entries below describe bounded policy-specific support; neither status establishes complete compatibility or live runtime verification.

- **function.ADDRESS** (registered; ExcelDocumentation): Fourth operand selects A1 or absolute/relative R1C1 axes. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.CEILING.MATH** (registered; ExcelDocumentation): Every nonzero mode rounds negative values away from zero. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.RANK** (registered; ExcelDocumentation): Shared documented RANK.EQ order, ties, absent-number and validated array/reference sample handling. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.COVARIANCE.S** (registered; ExcelDocumentation): Positional numeric-pair filtering and #DIV/0 for fewer than two valid pairs. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.NORM.S.DIST** (registered; ExcelDocumentation): Cumulative selector chooses the shared CDF or PDF kernel, including negative variates. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.ROMAN** (registered; ExcelDocumentation): Boolean form TRUE selects classic style and FALSE selects simplified style. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.IPMT** (registered; ExcelDocumentation): Four-to-six-argument signature with future value, timing and zero-rate interest handling. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.PPMT** (registered; ExcelDocumentation): Four-to-six-argument signature with future value, timing and zero-rate principal handling. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.BIN2DEC** (registered; ExcelDocumentation): Strict binary digit validation in the documented input domain. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.DELTA** (registered; ExcelDocumentation): Omitted second operand defaults to numeric zero. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.IMAGINARY** (registered; ExcelDocumentation): Numeric complex imaginary coefficient result. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.IMREAL** (registered; ExcelDocumentation): Numeric complex real coefficient result. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.TRANSPOSE** (registered; ExcelDocumentation): Typed numbers, exact text/UTF-16, booleans and cell errors retain shape; reference blanks become zero. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established. Worksheet footprints additionally require DynamicSpill and its static-geometry/read-only limits.
- **function.SUBTOTAL** (registered; ExcelDocumentation): Reference aggregation excludes nested parsed SUBTOTAL formulas; explicit sparse filtered-row flags apply to all codes and manually hidden flags to codes 101..111, with formula/visibility dependency guards. Limits: Current file adapters do not retain row visibility; callers must declare relevant visibility through Worksheet::set_row_visibility. Default pinned reference refusal is unchanged.
- **function.DATE** (registered; ExcelDocumentation): Documented negative-year #NUM and 1900 fictitious leap-slot normalization. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.DAYS** (registered; ExcelDocumentation): Numeric argument subtraction retains fractional serial differences. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.WEEKNUM** (registered; ExcelDocumentation): ISO return-type 21 handles previous and next week-year boundaries. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.WORKDAY** (registered; ExcelDocumentation): Signed fractional day counts truncate toward zero. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **function.CODE** (registered; ExcelDocumentation): Numeric first-character result in the admitted text policies. Limits: This is a bounded documented override; default source contracts are preserved and exhaustive Excel/POI parity is not established.
- **language.explicit_intersection** (partial; Default evaluation): Explicit @ parses as a dedicated implicit-intersection operator: scalar identity, array top-left, and unique horizontal/vertical reference intersection. Pinned SINGLE remains distinct. Limits: BIFF8 output cannot encode @ and explicitly rejects it. General array broadcasting and dynamic # reference syntax are not provided by this operator.
- **workbook.utf16_v1** (partial; Poi7321209OpenJdk21Unicode15): Lossless UTF-16 indexing and storage, including LEN/MID/LEFT/RIGHT/REPLACE results containing isolated surrogates. Limits: The Version 1 fixture profile uses the explicit POI UTF-16 proxy, not an observed Excel identity. Unicode scalar consumers may refuse isolated surrogates; casing remains source specific.
- **workbook.utf16_v2** (partial; ExcelCompatibilityV2Documented): Lossless LEN/MID/LEFT/RIGHT/REPLACE count each adjacent valid surrogate pair once; isolated surrogates, combining marks and selectors remain separate units. Limits: FIND/SEARCH retain pinned UTF-16 indexing. Case conversion uses the admitted OpenJDK 21 Unicode 15 ROOT table and contextual limits; no general Excel casing parity. The older unverified refusal policy remains unchanged.
- **workbook.array_results** (partial; DynamicSpill, ExcelDocumentation for typed TRANSPOSE): Typed static rectangular arrays publish anchor and owned neighboring cells in the read-only source-cell runtime; full footprint assertions read the actual retained memo. Limits: Default final results retain scalar intersection. General broadcasting/CSE, unbounded producers, incremental execution, mutable cache materialization and spill ownership export are unsupported.
- **language.spill** (partial; DynamicSpill, ExcelDocumentation for typed TRANSPOSE): Read-only source-cell runtime publication of typed rectangular static producers: array constants, TRANSPOSE, MMULT, MINVERSE, FREQUENCY and bounded IF/IFERROR/CHOOSE branches; ownership, blocking, child discovery and fresh-run edit invalidation. Limits: General broadcasting, CSE and # spill-reference syntax are absent. Names, extension arrays, TREND and computed-reference producers without bounded static geometry refuse. Incremental execution, mutable model cache recalculation and spill ownership export are unsupported; ad hoc array probes and external scalar cache readers refuse.
- **function.PERCENTILE.INC** (partial; ExcelDocumentation): Inclusive percentile endpoints and interpolation through the existing budgeted typed percentile kernel. Limits: Profile-conditioned unresolved alias resolution retains the existing percentile kernel/coercion limits; no exhaustive Excel parity claim. Absent from the default 256-entry registry; registered extensions retain precedence.
- **function.QUARTILE.INC** (partial; ExcelDocumentation): Typed quartile selector truncation into 0..4 and inclusive percentile calculation through the shared budgeted host. Limits: Only the documented inclusive form is added; general array broadcasting and exhaustive coercion parity are outside scope. Absent from the default 256-entry registry; registered extensions retain precedence.
- **function.RANK.EQ** (partial; ExcelDocumentation): Typed reference or validated rectangular array sample ranking, shared tie ranks, any nonzero ascending order, and #N/A for absent numeric members. Limits: Scalar or missing list operands are invalid; no general function broadcasting or exhaustive coercion parity. Absent from the default 256-entry registry; registered extensions retain precedence.

## Explicit capability gaps

- language.structured_refs
- function.LET
- function.LAMBDA
- function.FILTER
- function.UNIQUE
- function.SORT
- function.SORTBY
- function.SEQUENCE
- function.RANDARRAY
