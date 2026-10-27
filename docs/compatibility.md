# Compatibility

Poirot targets modern Excel. It is useful today for workbooks whose formulas and
inputs you know, including models you or an AI generate. Validate the outputs your
application relies on; support for a function does not guarantee every Excel edge
case behaves identically.

## Formula evaluation

| Area | Current support and limits |
| --- | --- |
| Excel functions | Broad support, with remaining gaps in individual functions, optional arguments, coercion and array behavior. The [builtin catalog](https://github.com/yiblet/poirot/blob/master/src/eval/functions/registry/catalog.rs) lists registered names and argument counts. |
| Dynamic arrays | Implemented, including functions such as `FILTER`, `SORTBY`, `UNIQUE` and `SEQUENCE`. Spill compatibility still has gaps. Competing spills use top-left anchor priority: row first, then column. This is a Poirot policy, not a verified Excel rule. |
| Shared formulas | Imported copied formulas become complete, independent formula sources. Shared storage is separate from legacy array-formula groups. |
| Legacy array formulas | **Unsupported.** Formulas entered with Ctrl+Shift+Enter, including fixed multi-cell formula groups, are outside current support. Dynamic-array support does not cover these formulas. See [known incompatibilities](#known-incompatibilities) for how files keep them. |
| Formula language | `LET`, `LAMBDA` and functions that require lambda expressions are unsupported. |
| Blanks, text and errors | Many common cases work, including a reference to a blank cell evaluating to `0`. Less common combinations of literal, referenced and array values still have compatibility gaps. |
| Older Excel versions | Modern Excel takes priority. Poirot does not promise exact behavior for every historical Excel version. |

## Internationalization

Internationalization is limited. Poirot is not fully compatible with Excel's
locale- and language-specific behavior. Configurable numeric text parsing does
not establish general locale compatibility; the evaluator does not infer the host
locale.

Phonetic text, legacy byte-counting functions and several language-specific text
conversions are unsupported. Deferred functions include `PHONETIC`, `ASC`, `DBCS`,
`JIS`, `BAHTTEXT`, `FINDB`, `LEFTB`, `LENB`, `MIDB`, `REPLACEB`, `RIGHTB` and
`SEARCHB`. Check text conversion, parsing and formatting against the languages and
locales your workbook uses.

## IO

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
cell's style as presentation metadata. `book.model().style_context` retains source
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

Test the evaluation mode and input changes your application uses, including
repeated edits with an overlay. Record evaluation policies, time, randomness,
numeric locale and date system when comparing results.

The [compatibility runner](https://github.com/yiblet/poirot/blob/master/crates/poirot-test-runner/README.md) provides
case-level checks. LibreOffice results are useful comparison evidence but do not
establish Excel compatibility. Known gaps remain visible; passing tests are not a
claim of complete coverage.

The [specification inventory](https://github.com/yiblet/poirot/blob/master/crates/poirot-test-excel-specs/test-specs/inventory.json)
and [source records](https://github.com/yiblet/poirot/blob/master/crates/poirot-test-excel-specs/test-specs/sources.json) track
documented expectations and research; they do not establish current runtime
support. Consult the catalog and run relevant cases against your checkout. The
runner's [baseline](https://github.com/yiblet/poirot/blob/master/crates/poirot-test-runner/baseline.json) records historical
outcomes; unchanged known gaps can still produce a successful normal run.

Every Excel incompatibility is a bug, including known gaps. Please
[report incompatibilities](https://github.com/yiblet/poirot/issues) with a small
reproduction, inputs, expected and observed results, Excel version, Poirot revision
and evaluation settings. Include import warnings for file losses. Fixes and
AI-written PRs are welcome; see [Contributing](../CONTRIBUTING.md).
