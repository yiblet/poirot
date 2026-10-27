# Excel compatibility

Poirot reads, edits, calculates and writes XLSX workbooks, and reads BIFF8 XLS
files (Excel 97–2003). It targets modern Excel, including dynamic arrays, with
partial compatibility.

Function availability, calculation results and file preservation need separate
checks. A supported function may have unsupported inputs or behavior, and saving
can omit workbook features such as charts and macros. Use the sections below to
check your model, then [validate it with representative inputs](#checking-your-workbook).

- [Supported functions](#supported-functions)
- [Formula evaluation](#formula-evaluation)
- [Calculation settings](#calculation-settings)
- [Internationalization](#internationalization)
- [File support and preservation](#file-support-and-preservation)
- [Checking your workbook](#checking-your-workbook)
- [Reporting and maintaining compatibility](#reporting-and-maintaining-compatibility)

## Supported functions

Poirot implements **494 of 521 function names (94.8%)** in Microsoft's
[alphabetical function index](https://support.microsoft.com/en-us/excel/excel-functions-alphabetical),
as counted on 2026-10-26. This counts names across Excel versions, including
aliases; it does not measure workbook compatibility.

| Excel category | Implemented / listed names | Availability |
| --- | ---: | ---: |
| Compatibility (legacy names) | 40 / 40 | 100% |
| Database | 12 / 12 | 100% |
| Date and time | 25 / 25 | 100% |
| Engineering | 54 / 54 | 100% |
| Financial | 55 / 55 | 100% |
| Statistical | 111 / 111 | 100% |
| Math and trigonometry | 80 / 80 | 100% |
| Lookup and reference | 37 / 40 | 92.5% |
| Information | 19 / 22 | 86.4% |
| Text | 47 / 50 | 94.0% |
| Logical | 11 / 19 | 57.9% |
| Add-in and Automation | 1 / 3 | 33.3% |
| Cube | 0 / 7 | 0% |
| Web | 2 / 3 | 66.7% |
| **Total** | **494 / 521** | **94.8%** |

### Remaining function gaps

The missing functions fall into two groups:

- **Bindings and functional programming:** `LET`, `LAMBDA`, `BYCOL`, `BYROW`,
  `MAKEARRAY`, `MAP`, `REDUCE`, `SCAN` and `ISOMITTED`. These let formulas name
  values, define functions or work with functions as arguments; Poirot does not
  yet support those capabilities.
- **Workbook data, application integrations and services:** `INFO`, `PHONETIC`,
  `GETPIVOTDATA`, `WEBSERVICE`, `STOCKHISTORY`, `TRANSLATE`, `DETECTLANGUAGE`,
  `IMAGE`, `CALL`, `REGISTER.ID`, `RTD`, `CUBEKPIMEMBER`, `CUBEMEMBER`,
  `CUBEMEMBERPROPERTY`, `CUBERANKEDMEMBER`, `CUBESET`, `CUBESETCOUNT` and
  `CUBEVALUE`. These need data or integrations that Poirot does not provide, such
  as stored phonetic
  annotations, PivotTable data or external services.

### Function-specific limits

These available functions have restrictions that may affect your workbook:

| Functions | Supported behavior and limits | Tests |
| --- | --- | --- |
| `AGGREGATE` | Supports `function_num` 1–19, `options` 0–7, references, arrays and arrays of `k` values for rank, percentile and quartile functions. Three-dimensional references are refused. Filtered-out rows are excluded from references for every option; agreement with Excel for this rule is unverified. | [Aggregate tests](https://github.com/yiblet/poirot/blob/master/tests/eval_aggregate.rs) |
| `SHEETS`, `ISFORMULA`, `FORMULATEXT` | `SHEETS` counts worksheets; chart, macro and dialog sheets are unsupported. `ISFORMULA` and `FORMULATEXT` inspect stored or scenario-edited formula text without calculating the referenced formula, and update when that text changes. | [Formula metadata tests](https://github.com/yiblet/poirot/blob/master/tests/eval_formula_metadata.rs) |
| `CELL` | Requires an explicit reference; ranges use the upper-left cell. Supports `address`, `row`, `col`, `contents`, `type`, `prefix`, `format`, `color` and `parentheses`. Format requests accept recognized format strings only; `parentheses` always returns 0, even for formats using parentheses. Other formats, filename, protection and width are unsupported. | [Cell information tests](https://github.com/yiblet/poirot/blob/master/tests/eval_cell_information.rs) |
| `GROUPBY` | Supports built-in aggregation functions and lists of them supplied with `HSTACK` or `VSTACK`. User-defined lambdas are unsupported. | [Grouping tests](https://github.com/yiblet/poirot/blob/master/tests/eval_grouping.rs) |
| `PIVOTBY` | Groups rows and columns with multiple grouping fields, one value column and one built-in aggregation function, such as `SUM`. Supports totals, sorting, filtering and `PERCENTOF` relative modes 0–4. `field_headers=0` uses all rows as data; `1` skips the first row. Automatic header detection requires one field per axis. Output field headers, multiple aggregation functions and callable values are unsupported. | [Pivot tests](https://github.com/yiblet/poirot/blob/master/tests/eval_pivotby.rs) |
| `FILTERXML` | XPath paths with simple predicates return a vertical array; numeric/date text uses the evaluation locale. Unrestricted XPath, DTDs and namespace prefixes that change meaning within a document are unsupported. Input, query and output limits apply even without application quotas; see the [query contract](https://github.com/yiblet/poirot/blob/master/crates/poirot-functions/src/text/xml/query.rs). | [XML tests](https://github.com/yiblet/poirot/blob/master/tests/eval_filterxml.rs) |

## Formula evaluation

| Feature | Supported behavior and limits | Tests or API |
| --- | --- | --- |
| Cell and range references | Supports single cells, ranges, sheet-qualified references, whole rows, whole columns and defined names. Excel tables and structured references such as `Sales[Amount]` are unsupported. | [Reference tests](https://github.com/yiblet/poirot/blob/master/tests/eval_reference_ranges.rs), [name tests](https://github.com/yiblet/poirot/blob/master/tests/eval_qualified_names.rs) |
| Three-dimensional references | Supports aggregates such as `SUM(Sheet1:Sheet3!A1)`, `AVERAGE` and `COUNT`. Reversed sheet endpoints stop evaluation. If only the first endpoint is missing, the reference selects the first worksheet; a missing last endpoint stops evaluation. | [Sheet-span tests](https://github.com/yiblet/poirot/blob/master/tests/eval_sheet_spans.rs) |
| Dynamic arrays | Implements `FILTER`, `SORT`, `SORTBY`, `UNIQUE`, `SEQUENCE`, `RANDARRAY`, `HSTACK`, `VSTACK`, `TAKE`, `DROP`, `CHOOSECOLS`, `CHOOSEROWS`, `EXPAND`, `TOCOL`, `TOROW`, `WRAPCOLS`, `WRAPROWS` and `TEXTSPLIT`. | [Array tests](https://github.com/yiblet/poirot/blob/master/tests/eval_internal_array_context.rs) |
| Worksheet spills and `#` references | Tracks spill ranges across edits and removes obsolete spill cells. Formulas and nonblank cells block spills. Competing spills favor the top-left anchor (row, then column); agreement with Excel for that priority is unverified. | [Spill tests](https://github.com/yiblet/poirot/blob/master/tests/dynamic_spills.rs), [spill-reference tests](https://github.com/yiblet/poirot/blob/master/tests/eval_spill_references.rs) |
| Explicit `@` and scalar array results | `@A1:A3` selects the cell in the formula's row. `ArrayResultPolicy::ScalarIntersection` returns a scalar final result while preserving intermediate arrays. | [Array policy](../src/eval/config/array_result_policy.rs) |
| Shared formulas | Imported shared formulas become independent, complete formula strings. Replacing the original anchor does not change its former followers. | [Shared-formula tests](https://github.com/yiblet/poirot/blob/master/tests/shared_formula_lifecycle.rs) |
| Legacy Ctrl+Shift+Enter arrays | Unsupported. XLS and XLSX imports retain different parts of these formulas; see [known incompatibilities](#known-incompatibilities). | [File-loss table](#known-incompatibilities) |
| External workbooks | Linked workbooks are not loaded or calculated. | [Reference failure tests](https://github.com/yiblet/poirot/blob/master/tests/eval_reference_host_contracts.rs) |
| Iterative circular calculation | Unsupported. Recursive cell reads can produce `#REF!`; circular spill dependencies can stop evaluation. | [Cell execution](../src/eval/session/cells/cell_attempt.rs) |
| Custom functions | Applications can register functions without replacing builtins. | [Custom functions](../README.md#calculate-the-way-your-application-needs) |
| Excel application features | Excel add-ins, VBA, application automation, PivotTable calculation and what-if data tables are unsupported. | — |

## Calculation settings

Set spreadsheet behavior with `EvaluationPolicy` and supply time, randomness,
locale and limits through `EvaluationContext`. The [evaluation guide](evaluation.md)
shows how to configure them.

| Setting or input | Effect on results |
| --- | --- |
| Array results | `ArrayResultPolicy::DynamicSpill` is the default. `ScalarIntersection` returns a scalar final result. |
| Saved spill results | `ProducerPolicy::Compute` calculates spill formulas. `TrustSaved` uses their saved results, which can remain stale after input or context changes. |
| Clock and randomness | `as_of_time` fixes the clock; `randomness_seed` controls repeatable draws. Change them when you need fresh values. Excel's recalculation schedule and random sequence are not reproduced. |
| Date system | The workbook selects `Excel1900` or `Excel1904`. Compare date serials using the same system, including Excel's fictitious 1900-02-29 (serial 60). |
| Blanks, empty text and missing arguments | These are distinct inputs and can produce different results. Test direct literals, referenced cells and array members when your model uses them. |
| Numbers | Values use binary64 floating point. Choose tolerances appropriate to your model when comparing numeric results with Excel; compare value types and errors exactly. |
| Calculation mode | Direct and incremental evaluation use the workbook plus scenario edits. An overlay reuses unaffected results; test the sequence of edits your application performs. |
| Limits | Work quotas and parser or execution limits can stop calculation. Parser defaults allow 1,024 expression-parsing steps and nesting depth 128. See [resource configuration](resources.md). |

Worksheet errors such as `#DIV/0!` are `CellValue::Error` results.
Operational errors, such as `EvaluationError::UnsupportedCapability` or exhausted
budgets, stop evaluation and cannot be caught by `IFERROR`.

Unknown functions can yield `#NAME?`, and invalid formula syntax can yield
`#VALUE!` during cell evaluation. Check calculated error values as well as Rust
errors when validating results.

## Internationalization

Use English function names and English month names. Numeric separators, currency
and percent symbols, date order and fixed UTC offset are configured explicitly;
Poirot does not infer the host locale.

| Area | Supported behavior and limits |
| --- | --- |
| Numeric text | `NumberLocale` configures decimal, grouping and time separators, currency and percent symbols. It does not select display formatting. |
| Date/time text | `DateOrder` selects month/day/year or day/month/year for slash dates. Fixed UTC offsets do not model daylight-saving transitions. |
| Sheet and defined names | Exact spellings match. Case-insensitive matching supports ASCII and Latin-1 letter pairs. Other comparisons can be unresolved: checked lookup of `σ` against a sheet named `Σ` returns an error; `sheet_by_name` returns `None`. |
| Text indexing | Ordinary text functions count a surrogate pair as one character, following Excel Compatibility Version 2; `LEN("😀")` is 1. |
| Text storage | Excel UTF-16 text is retained exactly. Language-specific comparison and case conversion still need validation. |
| Number-format display | Format rendering is separate from calculation. Workbook layout and font rendering are unsupported. |

`LENB`, `LEFTB`, `RIGHTB`, `MIDB`, `FINDB`, `SEARCHB` and `REPLACEB` default to
`ByteTextLanguage::NonDbcs`, counting UTF-16 code units: `LENB("😀")` is 2.
Selecting `Dbcs` stops evaluation because Japanese, Chinese and Korean double-byte
indexing is unsupported.

`ASC`, `DBCS` and its `JIS` alias convert Japanese ASCII/kana widths, including
voiced kana. They leave unrelated scripts unchanged and do not implement
Chinese/Korean or Windows code-page conversion.

`BAHTTEXT` produces Thai currency text for magnitudes below 2^53 baht, rounding
to the nearest satang with ties away from zero. It uses เอ็ด for a final one only
after nonzero tens. Agreement with Excel for these wording and rounding rules
remains unverified.

## File support and preservation

Poirot detects XLSX and XLS from file contents. Saving creates a new XLSX package
with the supported content below; original XML and unmodeled features are omitted.

| Format | Read | Write | Limits |
| --- | --- | --- | --- |
| XLSX | Yes | Yes | Supports the cell data and styles described below. |
| XLS (BIFF8, Excel 97–2003) | Yes | No | Reads scalar values, supported formula tokens and saved results; styles and number formats are not decoded. |
| XLS (BIFF5/7 or earlier) | No | No | Import refuses the file. |
| Encrypted XLS/XLSX | No | No | No decryption support. |
| Native snapshot | Yes | Yes | Poirot-only state; not an Excel interchange format. |

Reading and saving use stored formula results without recalculating them. Call
`Workbook::recalculate()` before saving when you need current results throughout
the workbook; calculating selected outputs leaves other saved results unchanged.

XLSX imports and exports preserve cell fonts, pattern fills, four-sided borders,
common alignment and number-format strings, including styled blanks. Theme and
indexed colors retain their source identities and tint; Poirot does not resolve
them to rendered colors.

Inspect `Workbook::file_metadata.import_warnings` for content that import changed
or omitted. Some losses have no warning; check the table below before relying on
an exported copy.

Input size quotas default to unlimited. Set import limits with `Workbook::load()`
and `files::ReadOptions` or `files::ResourceBudget`; configure export limits with
`files::WriteOptions`. See [resource configuration](resources.md) for defaults and
failure types.

Native snapshots retain source and calculation state. Older source schemas are
refused; preparation from an older engine revision is discarded, or refused under
`RequireCompatible`. See [restoring native state](evaluation.md#restore-native-state).

### Known incompatibilities

XLS admission errors are wrapped in `IoError::BiffPreflight`; version and XLS
encryption failures have a `BiffRepresentationError` source. Encrypted XLSX uses
`PreflightError::EncryptedXlsx`.

| File content | What Poirot does | What you see |
| --- | --- | --- |
| XLS from Excel 5.0/95 (BIFF5/7) or earlier | Refuses the file. | `BiffRepresentationError::UnsupportedBiffVersion` |
| Encrypted XLS (XOR, RC4 or CryptoAPI), including files encrypted with Excel's default password | Refuses the file; Poirot does not decrypt. | `BiffRepresentationError::EncryptedXls` |
| Password-protected (encrypted) XLSX | Refuses the file; Poirot does not decrypt. | `PreflightError::EncryptedXlsx` |
| Saving as XLS | Not available; every save writes XLSX. | No XLS save API |
| XLS styles and number formats | Not decoded; cells import with the default style. | Warning: `XLS raw number formats are not retained; ...` |
| XLS formula tokens Poirot cannot decode, including legacy array formulas entered with Ctrl+Shift+Enter | Keeps the saved value without a formula. | Warning: `Unsupported BIFF formula tokens on sheet N at <cell>` (`N` counts from 0) |
| XLSX legacy array formulas (`t="array"`) | Keeps the formula at the top-left cell and treats its range as a dynamic-array spill. Saves write it as a dynamic-array formula. | No warning |
| Shared formulas | Every cell gets an independent formula. Saves write independent formulas unless you supply shared provenance. | No warning |
| Charts, drawings, macros, conditional formats, data validation, external links, merged cells and other unmodeled parts | Omitted. Original XML and unknown attributes are not kept. | Warning: `XLSX charts, macros, drawings, ...` or `XLS charts, macros, drawings, rich styles, ...` |
| Gradient fills, diagonal borders, named styles and row/column style inheritance (XLSX) | Omitted or flattened into cell styles. | Warning: `Unsupported XLSX style records omitted ...` or `XLSX row/column style inheritance is omitted` |
| Rich text runs | Imported as plain text. | No separate warning |
| Print areas, print titles and filter ranges (XLSX) | Omitted. | No warning |
| Filtered-out rows (`RowVisibility::filtered_out`) | Saves write filtered-out rows as hidden; reads mark hidden XLSX and XLS rows as manually hidden. After a reopen, `SUBTOTAL` 1-11 and `AGGREGATE` options that keep hidden rows include these rows again. | No warning |

## Checking your workbook

1. Check the functions, references and file features your model uses against this
   guide. Keep the original file if you need features that export omits.
2. Capture expected results in Excel for representative inputs and boundary
   cases. Record the Excel version/build, platform, date system, calculation
   settings and locale, and recalculate before capturing results.
3. Load with Poirot, inspect import warnings and calculate the outputs you need.
   Compare value types, errors and array shapes as well as numbers.
4. Repeat the ordered edits your application makes, including changes to branches,
   names and spill ranges. Control time and randomness, and use numeric tolerances
   appropriate to the model.
5. Export, reopen and compare results and required file features. Use `all()` on
   a scenario, or workbook recalculation, when every saved result must be current.

### Run and interpret repository checks

The [compatibility runner](https://github.com/yiblet/poirot/blob/master/crates/poirot-test-runner/README.md)
lists cases, runs a selection and generates reports:

```sh
cargo run -p poirot-test-runner -- list --source json --filter lookup --cases
cargo run -p poirot-test-runner -- run --source json --filter lookup --strict
cargo run -p poirot-test-runner -- run --format json > /tmp/poirot-compat-results.jsonl
cargo run -p poirot-test-runner -- report /tmp/poirot-compat-results.jsonl --html /tmp/poirot-compat-report.html
```

JSON scenarios exercise direct and incremental evaluation. The FODS corpus
compares LibreOffice test fixtures, including cases outside Poirot's supported
features; FODS is not an application workbook input format.

| Outcome | Meaning |
| --- | --- |
| `passed` | The case met its recorded expectation. |
| `mismatch` | The result, type, error or predicate differed from the expectation. |
| `blocked` | A missing capability, unsupported test context or resource limit prevented comparison. |
| `skipped` | The case was not executed. |
| `harness_error` | The test infrastructure could not produce a valid result. |

Normal runs fail on regressions but can succeed with unchanged known mismatches,
blockers or skips. `--strict` fails on those outcomes too; neither mode updates the
baseline. Read case outcomes before interpreting a successful command.

### Compatibility evidence

Use these repository sources to investigate a result:

| Source | What it provides |
| --- | --- |
| [Function catalog](../src/eval/functions/registry/catalog.rs) | Registered names, aliases and argument counts. |
| [Authored scenarios](https://github.com/yiblet/poirot/tree/master/crates/poirot-test-runner/corpus/json) | Expected results, ordered edits, tolerances and evidence for individual cases. |
| [Workbook specifications](https://github.com/yiblet/poirot/blob/master/crates/poirot-test-excel-specs/test-specs/inventory.json) and [source records](https://github.com/yiblet/poirot/blob/master/crates/poirot-test-excel-specs/test-specs/sources.json) | Documented expectations and their sources; specifications may precede implementation. |
| [Runner baseline](https://github.com/yiblet/poirot/blob/master/crates/poirot-test-runner/baseline.json) | Recorded outcomes used to detect regressions. |
| [Comparison evidence](https://github.com/yiblet/poirot/tree/master/crates/poirot-test-runner/evidence) | Observations for the recorded engine, revision and cases. |

Excel observations establish results for the tested inputs and settings.
Apache POI and LibreOffice provide additional comparisons; agreement with them
does not establish agreement with Excel. The cases are not a representative
sample from which to estimate the percentage of compatible workbooks.

Read this guide at the Poirot tag or commit you use. Repository test and evidence
links point to `master`; select the matching revision when comparing results.

## Reporting and maintaining compatibility

[Report an incompatibility](https://github.com/yiblet/poirot/issues) with:

- A minimal formula or workbook and the ordered edits that reproduce it.
- Expected and actual values, types, errors and spill shapes, with evidence for
  the expected result and any numeric tolerance.
- Excel version/build/platform and Poirot tag or commit.
- Evaluation mode, policies, clock/seed, locale, date system and resource limits.
- Import warnings and lost file features, for preservation bugs.

For compatibility fixes, update the relevant behavior or limitation here and add
an independently supported regression test. Changing a known-gap baseline does
not fix the underlying behavior; see [Contributing](../CONTRIBUTING.md).
