# Compatibility

Poirot targets modern Excel. It is useful today for workbooks whose formulas and
inputs you know, including models you or an AI generate. Validate the outputs your
application relies on; support for a function does not guarantee every Excel edge
case behaves identically.

## Formula evaluation

| Area | Current support and limits |
| --- | --- |
| Excel functions | Broad support, with remaining gaps in individual functions, optional arguments, coercion and array behavior. The [builtin catalog](../crates/poirot-eval/src/registry/catalog.rs) lists registered names and argument counts. |
| Dynamic arrays | Implemented, including functions such as `FILTER`, `SORTBY`, `UNIQUE` and `SEQUENCE`. Spill compatibility still has gaps. Competing spills use top-left anchor priority: row first, then column. This is a Poirot policy, not a verified Excel rule. |
| Legacy array formulas | **Unsupported.** Formulas entered with Ctrl+Shift+Enter, including fixed multi-cell formula groups, are outside current support. Dynamic-array support does not cover these formulas. |
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

## Files and saved results

Poirot reads and writes XLSX and XLS. Saving preserves the features represented
by its workbook model; it does not guarantee a lossless round trip of every Excel
feature. Review `Workbook::file_metadata.import_warnings` for unsupported or
altered content. See [file handling](../crates/poirot-io/README.md).

Saved formula results can be stale. Reading or writing a file does not itself
recalculate its formulas. Formula correctness and preservation of workbook
presentation are separate concerns.

Charts, macros, drawings, rich styles, data validation and merge layout are not
preserved. Number-format strings are represented, but visual fidelity is limited.
External relationships are neither followed nor preserved.

## Checking your workbook

Test the evaluation mode and input changes your application uses, including
repeated edits with an overlay. Record evaluation policies, time, randomness,
numeric locale and date system when comparing results.

The [compatibility runner](../crates/poirot-test-runner/README.md) provides
case-level checks. LibreOffice results are useful comparison evidence but do not
establish Excel compatibility. Known gaps remain visible; passing tests are not a
claim of complete coverage.

The [specification inventory](../crates/poirot-test-excel-specs/test-specs/inventory.json)
and [source records](../crates/poirot-test-excel-specs/test-specs/sources.json) track
documented expectations and research; they do not establish current runtime
support. Consult the catalog and run relevant cases against your checkout. The
runner's [baseline](../crates/poirot-test-runner/baseline.json) records historical
outcomes; unchanged known gaps can still produce a successful normal run.

Every Excel incompatibility is a bug, including known gaps. Please
[report incompatibilities](https://github.com/yiblet/poirot/issues) with a small
reproduction, inputs, expected and observed results, Excel version, Poirot revision
and evaluation settings. Include import warnings for file losses. Fixes and
AI-written PRs are welcome; see [Contributing](../CONTRIBUTING.md).
