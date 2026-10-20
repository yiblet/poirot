# poirot-text

Count, slice, and compare text for spreadsheet functions.

Use `TextSemanticsPolicy` to select text-counting rules, or pass `Units` directly
to helpers such as `length` and `slice`. Supported units include UTF-8 bytes,
UTF-16 code units, and Unicode scalar values. Text is not normalized.

`fold` provides Unicode 16 simple case-insensitive comparison without character
expansion. Its UTF-16 adapter preserves unpaired surrogates. Spreadsheet text
function implementations live in
[poirot-functions-text](https://docs.rs/poirot-functions-text).
