# Lossless text

`CellValue::Text(String)` remains available. `CellValue::TextUtf16(ExcelText)` also preserves exact UTF16 code units, including isolated surrogates. Exhaustive matches must handle both variants. Use `text_ref()` and its unit view for unit-based operations; request a fallible UTF8 projection only when the consumer requires Unicode scalar values. Neither replacement characters nor saved formula values substitute for unsupported evaluation.

Text result and cache identity compare exact UTF16 units across the two storage representations. Retained-byte admission still accounts for physical storage.

`EvaluationFailure::ResourceLimit` distinguishes a source or adapter resource refusal from spreadsheet errors and unsupported capabilities. Exhaustive failure matches must handle this variant. `FormatError` now exposes a typed `kind`: invalid format, unsupported fidelity or resource limit. Database text-consuming ports return `DatabaseError`, preserving operational failures rather than converting them into spreadsheet errors. Callers must propagate those failures through their operational error channel.

Explicit text policies control unit and case semantics; existing core defaults remain unchanged. The OpenJDK21 Unicode15 policy uses the registered immutable case-mapping asset. Non-ASCII PROPER and case contexts outside the admitted sigma grammar remain unsupported. Unicode scalar consumers refuse isolated UTF16 surrogates operationally, including through IFERROR; unit-based operations can preserve those units successfully.

XLSX and BIFF8 cell/cache text paths preserve exact units within their admitted resource bounds. BIFF8 cached STRING and SST continuations have distinct decoding rules. Rich cached STRING records, non-BIFF8 fidelity, general external workbook/UDF NameX identities, general ATP export, XLS 3D/qualified-name output and array encoding remain unsupported. The bounded original add-in reconstruction covers EDATE, YEARFRAC and IFERROR identities with checked declarations, slots, scopes and arities; it does not establish general external-function support.



The documented Version 2 policy integrates lossless LEN/MID/LEFT/RIGHT/REPLACE
indexing into the evaluator. Adjacent valid high/low surrogate pairs count once;
isolated surrogates, combining marks and selectors remain independent. Version 1
unit slicing and replacement can preserve exact isolated-surrogate outputs.
FIND/SEARCH retain pinned UTF-16 indexing. Case conversion reuses the admitted
OpenJDK 21 Unicode 15 ROOT table with contextual limits, rather than claiming
complete Excel casing parity. See the [text contract](../crates/poirot-text/CONTRACT.md).
