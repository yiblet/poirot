# Excel compatibility

Poirot is an independent implementation of spreadsheet calculation and file IO.

## TL;DR

Poirot's strength is calculating XLSX workbooks: it implements **475 of 521 listed
Excel function names (91.2%)** under the default Excel policy. That includes every
listed financial, statistical, date/time, engineering and database function. See
[function coverage](#function-coverage) for the category breakdown and counting rules.

### Strengths

- **Broad formula coverage:** arithmetic, lookups, text, criteria and matrix
  calculations, alongside the complete function-name coverage in the categories
  above. Missing names include specialized text functions such as `RIGHTB` and
  `PHONETIC`, but also `LET`, `LAMBDA` and their consumers.
- **Modern array calculation:** 18 named array generation, selection, sorting and
  reshaping functions, with worksheet spills, `#` references and recalculation
  across edits.
- **XLSX workflows:** reading, editing, calculating and writing modeled workbook
  content, including ordinary cell styles. English formula names and explicit
  numeric/date settings provide the strongest supported path.

### Weaknesses

1. **Charts and presentation:** charts, drawings, macros, conditional formatting,
   data validation, merged cells and other unmodeled content are not preserved.
   Saving creates a new XLSX package; Poirot does not render workbook layout.
2. **Named Excel tables:** table objects and structured references such as
   `Sales[Amount]` have no supported calculation or preservation contract.
   Ordinary defined names and cell/range references are supported separately.
3. **Internationalization:** English formula syntax and English month names are
   the supported baseline. Latin-script data can use configurable numeric tokens
   and date order, but locale-specific comparison and case conversion need
   validation. Unicode storage includes non-Latin and right-to-left text; that
   does not establish language-specific formula, collation or display support.
   Treat RTL and other non-Latin language workflows as requiring extra validation,
   rather than assuming the English-language contract applies unchanged.
4. **Legacy Excel:** XLS support is limited to reading BIFF8 (Excel 97–2003), with
   incomplete formula-token decoding and no styles or number formats. Older XLS
   formats and XLS writing are unsupported. Legacy Ctrl+Shift+Enter array semantics
   are also unsupported, including in XLSX. The file format and formula features
   matter more than the year a workbook was authored.

Function availability is not a measured workbook success rate: **we have not
established a percentage of compatible Excel 2021 or other modern workbooks**.
Individual argument, reference and array contexts still need validation; calculation
and lossless file preservation are separate contracts.

This guide describes the current checkout. Read it at the tag or commit you
use. Packaged implementation links are relative to that revision. Source-only
tests and evidence link to the repository master branch; select the matching tag
or commit when comparing results. No complete Excel release/build conformance
matrix is maintained. “Modern Excel” is the target, not a claim of parity with
every Microsoft 365 update, platform or historical Excel version. Validate known models across the inputs your application uses.

- [TL;DR](#tldr)
- [Status and evidence](#status-and-evidence)
- [Function coverage](#function-coverage)
- [Formula evaluation](#formula-evaluation)
- [Calculation policies and observable differences](#calculation-policies-and-observable-differences)
- [Internationalization](#internationalization)
- [File support and preservation](#io)
- [Checking your workbook](#checking-your-workbook)
- [Reporting and maintaining compatibility](#reporting-and-maintaining-compatibility)

## Status and evidence

Function percentages count implemented names against a dated Excel function
index. Other areas give concrete counts of supported forms or behaviors; those
counts have no exhaustive Excel denominator and are not overall compatibility
percentages. The following terms describe implementation scope:

| Status | Meaning |
| --- | --- |
| Implemented | The stated behavior has an implementation and focused tests. This does not establish every Excel edge case. |
| Scoped | The row counts the implemented forms or behaviors and states the remaining restrictions. |
| Unsupported | The feature has no supported calculation or preservation contract. Import may still retain formula text or saved values; see the file-loss table. |
| Unverified | Poirot has behavior, but its agreement with Excel has not been established for the stated case. |

Calculation, file preservation and presentation are separate contracts. Successful
loading does not prove a formula can be recalculated. A saved formula result does
not prove that Poirot evaluated it. Correct calculated values do not prove that a
saved workbook retains its charts, formatting or other features.

Use these sources for different questions:

| Source | What it establishes | What it does not establish |
| --- | --- | --- |
| [Function catalog](../src/eval/functions/registry/catalog.rs) | Registered names, aliases, argument counts and dispatch metadata | Excel correctness for every argument or evaluation context |
| [Authored scenarios](https://github.com/yiblet/poirot/tree/master/crates/poirot-test-runner/corpus/json) | Expected values, ordered edits, numeric tolerances and evidence for individual cases | Coverage of inputs or features absent from the scenarios |
| [Workbook specifications](https://github.com/yiblet/poirot/blob/master/crates/poirot-test-excel-specs/test-specs/inventory.json) and [source records](https://github.com/yiblet/poirot/blob/master/crates/poirot-test-excel-specs/test-specs/sources.json) | Documented expectations and their research sources | Current runtime support merely because a specification exists |
| [Runner baseline](https://github.com/yiblet/poirot/blob/master/crates/poirot-test-runner/baseline.json) | Historical case outcomes, including known mismatches and blockers | A fresh run, an Excel certification, or a coverage percentage |
| [Comparison evidence](https://github.com/yiblet/poirot/tree/master/crates/poirot-test-runner/evidence) | Results and provenance for the recorded engine, revision and selection | Parity with another engine/version or the current checkout |

Excel observations should identify the Excel version/build, platform and relevant
settings. Microsoft documentation supports documented contracts; an observation
supports the particular case tested. Apache POI source contracts and LibreOffice
fixtures are useful independent evidence, but agreement with either does not by
itself establish Excel compatibility. Conflicting references need review rather
than majority voting among engines.

## Function coverage

**475 / 521 = 91.2% function availability**, measured against Microsoft's
[alphabetical function index](https://support.microsoft.com/en-us/excel/excel-functions-alphabetical)
on 2026-10-26. Counts include legacy spellings as separate names, split combined
entries such as `FIND, FINDB`, and normalize the index's `BETA.INVn` typo to
`BETA.INV`. The index spans Excel versions and platforms; it is the denominator
for this snapshot, not a fixed total for every Excel release.

| Excel category | Implemented / listed names | Availability |
| --- | ---: | ---: |
| Compatibility (legacy names) | 40 / 40 | 100% |
| Database | 12 / 12 | 100% |
| Date and time | 25 / 25 | 100% |
| Engineering | 54 / 54 | 100% |
| Financial | 55 / 55 | 100% |
| Statistical | 111 / 111 | 100% |
| Math and trigonometry | 79 / 80 | 98.8% |
| Lookup and reference | 35 / 40 | 87.5% |
| Information | 16 / 22 | 72.7% |
| Text | 36 / 50 | 72.0% |
| Logical | 11 / 19 | 57.9% |
| Add-in and Automation | 1 / 3 | 33.3% |
| Cube | 0 / 7 | 0% |
| Web | 0 / 3 | 0% |
| **Total** | **475 / 521** | **91.2%** |

The lookup row combines the index's “Lookup and reference” and “Look and reference”
labels. The [catalog](../src/eval/functions/registry/catalog.rs) contains **477 unique
names** in one unified builtin catalog. Two names,
`SINGLE` and `ANCHORARRAY`, are outside the index and excluded from the numerator.
`SUPPORTED_FUNCTIONS` exposes all 477 registered names. See [registration tests](https://github.com/yiblet/poirot/blob/master/tests/eval_excel_builtin_policy.rs).

These counts measure implemented dispatch, including aliases, rather than complete
argument or edge-case conformance. For example, `GROUPBY` has an implementation
restricted to typed built-in reductions. A 100% category means every listed name
has an implementation, with individual behavioral limits still applicable.

When registrations change, recount the unified catalog and intersect its
unique names with the dated index using the rules above. Update the total,
category rows and affected scope counts together.

## Formula evaluation

These are feature-level boundaries. For an individual function, inspect its
catalog entry and relevant scenarios; examples below are not exhaustive function
lists. No entire function family is certified as Excel-equivalent.

| Feature | Status | Current behavior and limits | Implementation or checks |
| --- | --- | --- | --- |
| Registered Excel functions | 475 / 521 (91.2%) | **477 callable names**, including two outside the measured Excel index; see [function coverage](#function-coverage). Optional arguments, coercion, errors and array handling still need case-level validation. | [Catalog](../src/eval/functions/registry/catalog.rs), [scenarios](https://github.com/yiblet/poirot/tree/master/crates/poirot-test-runner/corpus/json) |
| Cell and range references | Scoped: 6 forms | Single cells, bounded ranges, sheet-qualified references, whole rows, whole columns and defined names have implementations. Names and reference-producing functions need workbook context; parser acceptance alone is insufficient. | [Reference tests](https://github.com/yiblet/poirot/blob/master/tests/eval_reference_ranges.rs), [name tests](https://github.com/yiblet/poirot/blob/master/tests/eval_qualified_names.rs) |
| Three-dimensional references | Scoped: 3 aggregate examples | Tests establish `SUM`, `AVERAGE` and `COUNT` across sheet spans, including `SUM(Sheet1:Sheet3!A1)`. This is a count of demonstrated aggregate names, not the total supported consumers. Reversed or unresolved endpoints have source-specific behavior or operational refusals; do not infer full Excel support from the ordinary aggregate case. | [Sheet-span tests](https://github.com/yiblet/poirot/blob/master/tests/eval_sheet_spans.rs), [reference resolution](../src/eval/vm/reference_atoms.rs) |
| Dynamic-array functions | Scoped: 18 functions | `FILTER`, `SORT`, `SORTBY`, `UNIQUE`, `SEQUENCE`, `RANDARRAY`, `HSTACK`, `VSTACK`, `TAKE`, `DROP`, `CHOOSECOLS`, `CHOOSEROWS`, `EXPAND`, `TOCOL`, `TOROW`, `WRAPCOLS`, `WRAPROWS` and `TEXTSPLIT` are implemented. This counts the named generation, selection, sorting and reshaping functions; other functions can also return arrays. Array dimensions, broadcasting, errors and consuming functions require separate checks. | [Catalog](../src/eval/functions/registry/catalog.rs), [array tests](https://github.com/yiblet/poirot/blob/master/tests/eval_internal_array_context.rs) |
| Worksheet spills and `#` references | Scoped: 6 behaviors | Publication, authored-cell blocking, resizing, obsolete-child cleanup, overlap ownership and `#` references have implementations. Authored formulas and nonblank values block incoming spills. Ownership and shape are tracked across edits. Competing spills favor the top-left anchor (row, then column); agreement with Excel for that priority is **unverified**. | [Spill tests](https://github.com/yiblet/poirot/blob/master/tests/dynamic_spills.rs), [spill-reference tests](https://github.com/yiblet/poirot/blob/master/tests/eval_spill_references.rs) |
| Explicit `@` and scalar array results | Scoped: 2 mechanisms | Explicit intersection and scalar result projection are implemented. `@A1:A3` selects in the formula's row. Scalar result policy projects the final result while preserving intermediate arrays. This does not implement legacy Ctrl+Shift+Enter semantics. | [Evaluation guide](evaluation.md), [array policy](../src/eval/config/array_result_policy.rs) |
| Shared formulas | Implemented | Imported copied formulas become independent, complete formula sources. The compiler can reuse templates without retaining the shared file encoding. | [Lifecycle tests](https://github.com/yiblet/poirot/blob/master/tests/shared_formula_lifecycle.rs), [file tests](https://github.com/yiblet/poirot/blob/master/tests/io_shared_formulas.rs) |
| Legacy Ctrl+Shift+Enter array groups | Unsupported | Fixed multi-cell array groups are not evaluated with legacy Excel semantics. XLS and XLSX imports handle these differently, including silent conversion for XLSX; see [known incompatibilities](#known-incompatibilities). | [File-loss table](#known-incompatibilities) |
| `LET`, `LAMBDA` and lambda consumers | Unsupported | Local bindings, lambda expressions and functions requiring lambdas have no supported evaluation contract. | [Catalog](../src/eval/functions/registry/catalog.rs) |
| External workbook calculation | Unsupported | Poirot does not follow external relationships or load linked workbooks. Preserved text or caches do not establish live external calculation. | [File handling](https://github.com/yiblet/poirot/blob/master/crates/poirot-io/README.md), [reference failure tests](https://github.com/yiblet/poirot/blob/master/tests/eval_reference_host_contracts.rs) |
| Iterative circular calculation | Unsupported | Poirot does not provide Excel's iterative calculation solver. Recursive reads can produce `#REF!`; circular unpublished spill dependencies can stop evaluation. | [Cell execution](../src/eval/session/cells/cell_attempt.rs), [spill tests](https://github.com/yiblet/poirot/blob/master/tests/dynamic_spills.rs) |
| Custom functions | Implemented | Applications can register providers; existing builtins keep their implementations. An extension's behavior and external dependencies are the application's contract, not evidence of Excel add-in or VBA support. | [Application API](../README.md#calculate-the-way-your-application-needs) |

Unlisted Excel features have no compatibility claim in this guide. In particular,
do not infer support for Excel tables/structured references, pivot calculations,
what-if data tables, VBA or application automation from ordinary cell evaluation.
Check the file-loss contract before importing a workbook containing those features.

## Calculation policies and observable differences

Default policies target documented Excel behavior, but several independent choices
affect results. Record the complete policy and context when comparing engines.
The [evaluation guide](evaluation.md) explains how builders accept them.
The defining [policy](../src/eval/config/evaluation_policy.rs) and
[context](../src/eval/config/evaluation_context.rs) types list the independent
settings; changing one does not select a complete Excel-version profile.

| Area | Default behavior or boundary | Consequence for an application |
| --- | --- | --- |
| Arrays | `ArrayResultPolicy::DynamicSpill`; `ScalarIntersection` is an explicit alternative | Validate both the anchor value and spill footprint, including blockers and overlap changes after edits. |
| Saved spill producers | `ProducerPolicy::Compute`; `TrustSaved` deliberately consumes saved producer results | Trusted results may stay stale after input or context changes. They must not be counted as recalculated results. |
| Clock and randomness | The context captures time and a random seed. Unchanged consumed context permits overlay reuse | Change `as_of_time` or `randomness_seed` when fresh clock-dependent results or draws are required. Do not assume Excel's recalculation scheduling or random sequence. |
| Date system | The workbook selects `Excel1900` or `Excel1904` | Compare serials using the same epoch, including serial 60 and boundary cases. Date parsing also depends on explicit date order. See [date tests](https://github.com/yiblet/poirot/blob/master/tests/eval_excel_date_policy.rs). |
| Blanks, empty text and missing arguments | These inputs are distinct; a reference to a blank cell commonly evaluates to zero | Test direct literals, referenced cells and array members separately. One passing scalar case does not validate coercion or error precedence in another context. |
| Numeric results | Numbers use binary64; function algorithms and error boundaries need individual evidence | There is no universal bit-for-bit or numerical tolerance guarantee against Excel. Record a justified tolerance per numeric comparison, and compare value kinds and errors exactly. |
| Formula-cache publication | Computed values are preserved when publishing formula caches | A live result and a saved formula cache are separate observations. See [publication tests](https://github.com/yiblet/poirot/blob/master/tests/formula_cache_materialization.rs). |
| Direct and incremental modes | Both calculate against source plus scenario edits; an overlay reuses unaffected results | Validate ordered edit sequences, formula replacement, context changes and spill resizing, not only a cold calculation. |
| Limits | Application quotas and structural/native guards can stop calculation | A resource refusal is not an Excel cell error or a compatibility success. Parser defaults include 1,024 expressions and depth 128, even when application quotas are omitted. See [parse policy](https://github.com/yiblet/poirot/blob/master/crates/poirot-parsing/src/formula/metadata.rs) and [resources](resources.md). |

Worksheet errors such as `#DIV/0!` are calculated `CellValue::Error` values.
Operational failures such as `EvaluationError::UnsupportedCapability` or budget
exhaustion stop evaluation and cannot be caught by `IFERROR`. Unknown functions
can instead yield `#NAME?`, and invalid formula syntax can yield `#VALUE!` during
cell evaluation. There is no single “unsupported feature” signal: inspect Rust
errors, calculated error values **and** import warnings. Absence of warnings does
not establish compatibility or preservation.

## Internationalization

Internationalization includes **36 / 50 listed text functions (72.0%)**,
**5 configurable numeric tokens** (decimal, grouping, currency, percent and time),
and **2 slash-date orders** (month/day/year and day/month/year). These measure
different capabilities; there is no combined locale-coverage percentage. Numeric
input parsing, date parsing, text units and display formatting are separate
settings; the evaluator does not infer the host locale. See
[`NumberLocale`](https://github.com/yiblet/poirot/blob/master/crates/poirot-parsing/src/number.rs) and
[`DateOrder`](../src/eval/config/date_order.rs).

| Area | Current contract and limits |
| --- | --- |
| Formula language | Do not assume localized Excel function names or UI formula syntax are accepted. Use the registered names and parser grammar. |
| Numeric text | `NumberLocale` selects explicit numeric input tokens. This does not configure general workbook language or display formatting. |
| Date/time text | Date order and a fixed UTC offset are explicit. Month names are English; fixed offsets do not model daylight-saving transitions. |
| Text indexing | Text semantics target documented Excel Compatibility Version 2; `LEN("😀")` is 1. See [text-indexing tests](https://github.com/yiblet/poirot/blob/master/tests/eval_text_semantics.rs). |
| Text storage | Excel UTF-16 text can be retained exactly. Storage fidelity does not establish locale-specific comparison, case conversion or indexing parity. |
| Number-format display | Format rendering is available separately from calculation; workbook layout and font rendering are not implemented. Test the formats and languages you use. |

Phonetic text, legacy byte-counting functions and several language-specific text
conversions are unsupported. Deferred functions include `PHONETIC`, `ASC`, `DBCS`,
`JIS`, `BAHTTEXT`, `FINDB`, `LEFTB`, `LENB`, `MIDB`, `REPLACEB`, `RIGHTB` and
`SEARCHB`.

## IO

| Format | Read | Write | Boundary |
| --- | --- | --- | --- |
| XLSX | Implemented | Implemented | Modeled content only; saving authors a fresh package. See losses below. |
| XLS (BIFF8, Excel 97–2003) | Scoped: 4 scalar value kinds + formulas | Unsupported | Reads numbers, text, booleans and errors, plus supported formula tokens and saved results. Formula-token coverage has no measured denominator; styles and number formats are not decoded. See [cell decoding](https://github.com/yiblet/poirot/blob/master/crates/poirot-io/src/xls/cells.rs), [text decoding](https://github.com/yiblet/poirot/blob/master/crates/poirot-io/src/xls/text.rs) and [formula decoding](https://github.com/yiblet/poirot/blob/master/crates/poirot-io/src/xls/formula/decompile.rs). |
| XLS (BIFF5/7 or earlier) | Unsupported | Unsupported | Import refuses the file. |
| Encrypted XLS/XLSX | Unsupported | Unsupported | No decryption contract; encrypted input is refused. |
| Native snapshot | Implemented | Implemented | Poirot state with schema/revision checks; not an Excel interchange format. |

Poirot reads XLSX and XLS (BIFF8, Excel 97-2003) and writes XLSX. It detects the
format from the file contents. Saving preserves the features represented by its
workbook model; it does not guarantee a lossless round trip of every Excel
feature. Review `Workbook::file_metadata.import_warnings` for unsupported or
altered content. See [file handling](https://github.com/yiblet/poirot/blob/master/crates/poirot-io/README.md).

Saved formula results can be stale. Reading or writing a file does not itself
recalculate its formulas. Formula correctness and preservation of workbook
presentation are separate concerns.

XLSX imports and exports preserve ordinary cell fonts, pattern fills, four-sided
borders, common alignment and number-format strings, including styled blanks.
`CellStyle::visual` shares a `VisualStyle` payload; evaluation outputs expose the
cell's style as presentation metadata. `book.style_context` retains source
theme bytes, indexed palettes and the default cell style. Colors retain their
ARGB, theme, indexed or automatic identity and tint; they are not resolved to
rendered colors. Native snapshots retain these styles and their context. Poirot
does not render workbook layout. External relationships are neither followed nor
preserved.

Reads are unlimited by default. Set input, ZIP and XLS limits with
`Workbook::load()` and `files::ReadOptions` or `files::ResourceBudget` (including
`max_cells`, the decoded XLS cell count). Saves enforce `files::WriteOptions`
(defaults: 32 MiB compressed package, 32 MiB per part, 128 MiB expanded total,
10,000 parts) before returning bytes or writing a file. Exceeded limits return a
typed `IoError`.

Native snapshots are a Poirot-only format, not an Excel file. Snapshots with an
older source schema are refused; preparation saved by an older engine revision is
discarded, or refused under `RequireCompatible`. See
[restore native state](evaluation.md#restore-native-state).

### Known incompatibilities

Errors below are the `PreflightError` detail inside `IoError::BiffPreflight`.

| File content | What Poirot does | What you see |
| --- | --- | --- |
| XLS from Excel 5.0/95 (BIFF5/7) or earlier | Refuses the file. | Error: `only BIFF8 (Excel 97-2003) XLS workbooks are supported` |
| Encrypted XLS (XOR, RC4 or CryptoAPI), including files protected only by Excel's default password `VelvetSweatshop` | Refuses the file; Poirot does not decrypt. | Error: `encrypted XLS workbooks are not supported` |
| Password-protected (encrypted) XLSX | Refuses the file; Poirot does not decrypt. | Error: `encrypted (password-protected) XLSX workbooks are not supported` (`PreflightError::EncryptedXlsx`) |
| Saving as XLS | Not available; every save writes XLSX. | No XLS save API |
| XLS styles and number formats | Not decoded; cells import with the default style. | Warning: `XLS raw number formats are not retained; ...` |
| XLS formula tokens Poirot cannot decode, including legacy array formulas entered with Ctrl+Shift+Enter | Keeps the saved value without a formula. | Warning: `Unsupported BIFF formula tokens on sheet N at <cell>` (`N` counts from 0) |
| XLSX legacy array formulas (`t="array"`) | Keeps the formula at the top-left cell and treats its range as a dynamic-array spill. Saves write it as a dynamic-array formula. | No warning |
| Shared formulas | Every cell gets an independent formula. Saves write independent formulas unless you supply shared provenance. | No warning |
| Charts, drawings, macros, conditional formats, data validation, external links, merged cells and other unmodeled parts | Omitted. Saves author a fresh XLSX package, so original XML and unknown attributes are not kept. | Warning: `XLSX charts, macros, drawings, ...` or `XLS charts, macros, drawings, rich styles, ...` |
| Gradient fills, diagonal borders, named styles and row/column style inheritance (XLSX) | Omitted or flattened into cell styles. | Warning: `Unsupported XLSX style records omitted ...` or `XLSX row/column style inheritance is omitted` |
| Rich text runs | Imported as plain text. | No separate warning |
| Print areas, print titles and filter ranges (XLSX) | Omitted. | No warning |
| Filtered-out rows (`RowVisibility::filtered_out`) | Files keep one hidden flag per row, and Poirot does not keep filter ranges. Saves write filtered-out rows as hidden, and reads mark every hidden XLSX or XLS row as manually hidden. After a reopen, `SUBTOTAL` 1-11 and `AGGREGATE` options that keep hidden rows include these rows again. | No warning |

## Checking your workbook

1. Inventory formulas, names, array groups, linked workbooks and file features.
   Check both the function catalog and the feature/file-loss tables. Keep the
   original file when preservation matters.
2. Capture expected results in Excel for representative inputs and boundary
   cases. Record Excel version/build/platform, workbook date system, calculation
   settings, locale and whether Excel recalculated before saving. Saved caches
   alone may be stale.
3. Load with Poirot and inspect `Workbook::file_metadata.import_warnings`.
   Recalculate the outputs you depend on using explicit policies and context.
   Check value types, errors and array shapes as well as numbers.
4. Repeat the application's ordered edits in direct and overlay modes, including
   changes that alter branches, names or spill footprints. Control time and
   randomness. Select numeric tolerances from the model's requirements and
   reference evidence.
5. Export, reopen and compare both results and required workbook features.
   Loading or saving never recalculates by itself. A selected-output calculation
   leaves unrelated saved formula results potentially stale; use `all()` on a
   scenario or workbook recalculation when a full calculation is required.

### Run and interpret repository checks

The [compatibility runner](https://github.com/yiblet/poirot/blob/master/crates/poirot-test-runner/README.md) can list cases,
run a selection, retain case-level JSONL and generate an HTML report:

```sh
cargo run -p poirot-test-runner -- list --source json --filter lookup --cases
cargo run -p poirot-test-runner -- run --source json --filter lookup --strict
cargo run -p poirot-test-runner -- run --format json > /tmp/poirot-compat-results.jsonl
cargo run -p poirot-test-runner -- report /tmp/poirot-compat-results.jsonl --html /tmp/poirot-compat-report.html
```

The authored JSON scenarios run through normal and incremental-overlay evaluation.
The FODS corpus checks pinned LibreOffice fixtures and their predicates. FODS is a
development comparison corpus, **not an additional supported workbook input
format**. It contains source-specific functions and unsupported fixture contexts;
its aggregate failure count is not an Excel incompatibility count.

Read the observation independently of its change from the baseline:

| Observation | Meaning |
| --- | --- |
| `passed` | The selected case met its recorded expectation under that case's comparison rules. |
| `mismatch` | Execution produced a different result, type, error or predicate outcome. |
| `blocked` | A missing capability, unsupported fixture context or resource boundary prevented a valid comparison. This is not a pass. |
| `skipped` | The case was explicitly not executed. This is not a pass. |
| `harness_error` | The test infrastructure could not produce a trustworthy result. |

An unchanged mismatch or blocker can still give a successful **normal** run.
`--strict` also fails unchanged gaps and skips. Normal runs never update the
baseline. Inspect the selected cases, authority, evidence, tolerances, observations
and baseline changes before interpreting a green command. Passing case counts
cannot be converted into “percent Excel compatible”: the corpora are not an
exhaustive or representative sample of Excel behavior.

## Reporting and maintaining compatibility

Every Excel incompatibility is a bug to resolve, including a known gap. Explicit
alternative policies describe choices available today; they do not exempt the
default Excel target from compatibility fixes.

[Report an incompatibility](https://github.com/yiblet/poirot/issues) with:

- A minimal formula or workbook and the ordered input edits needed to reproduce it.
- Expected and observed typed values, errors and spill shapes; include the source
  of the expectation and any numeric tolerance.
- Excel version/build/platform and Poirot tag or commit.
- Evaluation mode, policies, clock/seed, locale, date system and resource limits.
- Import warnings and the features lost after export, for file-preservation bugs.

Fixes and AI-written PRs are welcome; see [Contributing](../CONTRIBUTING.md).
Compatibility changes should update the relevant row here and add a focused
regression case with independent expected-result evidence. State remaining
restrictions and observable failure behavior. A new registration alone does not
justify changing a feature to “Implemented.” Baseline changes require review of
the underlying observations; accepting a known mismatch is not fixing it.
