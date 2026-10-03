# poirot-xls

Write bounded BIFF8 XLS workbooks from poirot's sparse model. The writer uses a
reusable CFB container and follows supported HSSF record layouts from Apache POI.
It does not call Java or evaluate formulas.

`XlsExportPlan::new(&workbook, policy)` validates the source and retains its borrow.
`to_buffer` serializes it; `write_file` writes the result after serialization.
`poirot-workbook::Spreadsheet::plan_xls_save` exposes the same plan in the composed
workflow. See [export planning](EXPORT_PLANS.md) for supported records, formula
representation and resource limits.

Unsupported formulas or fidelity requirements return typed errors rather than
replacing formulas with cached literals. Rich formatting, arbitrary opaque
records and general external/array formula encoding are outside scope. Import
belongs to `poirot-io`; calculation belongs to `poirot-eval`.
