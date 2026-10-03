# XLSX export resource policy

`plan_xlsx_export(&workbook, WriteOptions)` performs pure inspection and holds an
immutable workbook borrow for execution. The plan exposes source identity, source
payload bytes, cell count, selected XLSX format, preserve-cache policy, represented
features, and the exact options execution will use. It never recalculates formulas.
The represented features are formulas, typed values/caches, names, date system and
number-format strings; arbitrary workbook features remain outside the model.

`write_xlsx_to_buffer_with_options` and `write_xlsx_with_options` route through
this plan. Facades can pass the policy through directly. Existing wrappers use
explicit defaults: 32 MiB compressed output and per-entry expansion, 128 MiB total
expansion, 10,000 entries, and no source-payload ceiling. Writer defaults are
independent of importer defaults. Set `max_source_bytes` to reject large UTF-8
source payloads before constructing backend cell structures. Worksheet literal and
cache text bytes also provide a safe lower bound for the entry budget.

Source payload accounting sums sheet names, text bytes, eight bytes for each
scalar cell, formulas, number-format strings, and name/formula bytes. It is neither
an XML-size prediction nor a memory estimate. Execution checks the generated ZIP
entry claims before expansion, counts patched XML before allocating each replacement,
checks actual resulting part sizes and total expansion, and checks compressed output
before returning or writing the destination. A refused file export does not write
the destination. Exact patch counting uses the same serializer as actual patching.

The pinned rust_xlsxwriter backend still materializes cell structures and the entire
generated compressed package before exact package sizes are available. ZIP expansion,
OPC inventory cloning, patch replacement and final compressed output can coexist in
memory. Counting avoids allocating an oversized patched part but still traverses XML
and allocates style metadata and individual escaped values. `max_output_bytes` is a
post-serialization result limit, not a bound on serializer allocation or peak RSS.
A streaming rewrite is deferred; these options must not be advertised as a resident
memory cap.

Repeated literal strings are emitted as inline worksheet text by ROI, even if the
intermediate backend package uses shared strings. Final source-byte accounting
therefore includes each occurrence. These limits do not bound peak process memory.
