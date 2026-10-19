# poirot-workbook

The workbook workflow behind Poirot's public API. Combines file loading, source
editing, formula evaluation, and saving.

Start with `Spreadsheet` for files, `Overlay` for changes over a borrowed workbook,
or `Evaluation` for repeated calculations. The `edit` and `translation_edit`
modules handle formula-aware structural changes. Use `save_xlsx()` or `save_xls()`
for configurable save builders. Evaluate and apply results before saving; saving
does not recalculate formulas.

These APIs are also exposed by the [root library](../../README.md).
