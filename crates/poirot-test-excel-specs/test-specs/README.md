# Case table and coverage inventory

`inventory.json` defines function, language, and workbook features with stable obligation IDs, scope, implementation status, and research applicability. Registration is not a compatibility claim. `registered` denotes only the default 262-entry public registry; `partial` can describe bounded opt-in support. Optional `implementation_scope` records execution policies, supported behavior and explicit limits; generated reports retain these separately from remaining unsupported features. Generic function facets still marked `requires_research` remain open even when individual examples have been specified. `coverage.json` lists every obligation and its linked assertions or missing status; `coverage.md` is the readout.

`cases.json`, stored at `../../poirot-test-runner/corpus/json/cases.json`,
is the only normative case table. The combined JSON runner executes each case
individually, preserving this schema and evidence. Each expected result has assertion-level evidence: `documented_example`, `documented_derivation`, `excel_observation`, or `unresolved`. A derivation states the specific rule and calculation; a source URL alone is insufficient. Observations require workbook identity, Excel version, settings, and a result artifact. Unresolved assertions remain visible and fail with an evidence explanation, rather than supplying a guessed expected result. No desktop Excel observations or ROI evaluator executions were used during authoring.

`sources.json` records the primary Microsoft sources, verification date, applicability, and case IDs. `generate.py` and `extend.py` build the reviewed initial matrices; authored inventory-driven scenarios live directly in `cases.json`; `assemble.py` links evidence and obligations and produces the one table. Maintenance commands are in [the package README](../README.md).

## Schema version 2

Each case contains `schema_version`, `id`, unique `name`, `category`, `rationale`, `compatibility`, `text_policy`, `source_urls`, `sheets`, `cells`, `names`, `settings`, and `steps`. IDs are contiguous and stable. Sheet indices are zero-based; cell addresses use A1 notation.

Cells contain either a typed `value` or a `formula`. Scalar types are `number`, `text`, `boolean`, `blank`, `error`, and `utf16`. Error values use Excel spelling, such as `#N/A`. UTF-16 values are arrays of exact 16-bit units, including isolated surrogates; no replacement-character normalization is allowed. Formula caches start blank and never supply the oracle.

Names have `name`, `formula`, and `sheet_scope` (`null` for global or a worksheet index for local). Settings support `date_system` (`1900` or `1904`), `currency` (`USD`), `text_policy` (`excel_v1` or `excel_v2`), `array_result_policy` (`scalar_intersection` or `dynamic_spill`), and explicit `context` fields: `as_of_time`, `randomness_seed`, `utc_offset_seconds`, and `date_order` (`MDY`/`DMY`). The fixture timestamp uses Serde's `SystemTime` object shape, for example `{"secs_since_epoch":1704110400,"nanos_since_epoch":0}`; execution carries the decoded instant unchanged. No host locale is read.

Every step has `edits` and one or more `assertions`. Edits use the same cell schema. Optional step `settings` change context/text/currency policy; context fields merge with earlier settings. Optional `names` replace the name list. Formula/name edits or explicit `recompile: true` replace the immutable source; value edits otherwise preserve it, clear retained results through the public API, and evaluate again.

An assertion has `target`, `expected`, `obligations`, and `evidence`, plus an optional absolute numeric `tolerance`. Evidence contains `kind`, `source_urls`, and `explanation`; optional `source_section` identifies the rule. Unresolved expectations may omit `expected`. Tolerance does not coerce types or hide spreadsheet errors.

An array expected value is a nonempty rectangular 2D array of typed scalar values, with `type: "array"`. The target is the top-left output cell. The runner evaluates and reads every expected footprint coordinate. This describes observable worksheet results, not an array return type invented for the scalar runtime API. Dynamic-spill cases cite their spill contract and compatible Excel version; unsupported spill behavior remains an explicit failure.

The JSON parser, fixture checks, evidence requirements, bounds, source indexes, duplicate detection, and inventory links are validated by `tools/validate_specs.py`. Generation is independent of ROI. IDs 0–1499 are the reviewed matrix prefix regenerated from code; append authored cases at IDs 1500 onward directly in the single table. Maintenance preserves that authored suffix and regenerates the source index and coverage. A blank scalar is `{ "type": "blank" }`, without a payload. The Rust schema uses typed structs and enums, rejects unknown case fields, validates cell addresses on deserialization, and models errors and settings as closed enums. Observation settings explicitly record `date_system`, `locale`, and numeric `compatibility_version` (1 or 2). The original matrix cases retain their inputs and mathematical expectations; the Version 2 schema adds evidence and obligation links without using observed ROI outputs.

Modern array-footprint cases explicitly set
`settings.array_result_policy` to `dynamic_spill` (`scalar_intersection` is the
optional default). Step settings can change the same policy. The runner reads
individual output coordinates from the public runtime cache, without deriving
or reconstructing results from expected arrays.

Case 1513 retains the published BESSELJ example's input and absolute tolerance,
but derives its expectation from the defining series in the same Microsoft
source. The example decimal 0.329925829 differs from J2(1.9) by approximately
1.0130761276e-7; it is not a nine-place rounding of the series. Run
`python3 tools/derive_besselj_oracle.py` from the crate directory to reproduce
the independent Decimal calculation at 70 and 90 digits using 100 terms.
`maintain.py` runs this check against the preserved authored suffix after
regeneration. The assertion records `documented_derivation` evidence; it does
not claim a native Excel observation or use ROI/libm as its oracle.

The fixture runner selects `ExcelDocumentation` independently of text and array
publication policies. Version 1 uses the explicit POI UTF-16 proxy; Version 2 uses
`ExcelCompatibilityV2Documented` for lossless LEN/MID/LEFT/RIGHT/REPLACE counting.
FIND/SEARCH and casing retain source-specific limits. DynamicSpill covers bounded
static producers, including explicit/bounded IF/IFS/SWITCH/CHOOSE selection and
exact information predicates under ExcelDocumentation. Singleton axes broadcast;
ordinary scalar consumers retain their own contracts. This does not provide
general function vectorization, CSE, # reference syntax or unbounded named/computed
producer inference. See the evaluator's dynamic-array contract for supported
incremental spill reuse and materialization; specification coverage remains
bounded to the authored cases.
SUBTOTAL visibility must be declared explicitly; file import does not currently
retain filtered/manually hidden metadata. These scopes describe implementation
capabilities, not native Excel observations. Authoring reports remain independent
of later runtime `cargo test` results.
