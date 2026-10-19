# poirot-text

Text counting and slicing rules used by formula evaluation. Supports explicit
policies for UTF-8 bytes, UTF-16 code units, Unicode scalars, and surrogate pairs.

Use `TextSemanticsPolicy` to select behavior, then the length, slicing, and
concatenation helpers to apply it. Text is not normalized. Unsupported source
policies or boundaries return errors rather than guessing compatibility.

Spreadsheet text functions live in
[poirot-functions-text](../poirot-functions-text/README.md).

`fold` provides Unicode 16 simple case-insensitive comparison. It does not expand
characters or normalize text, and its UTF-16 adapter preserves unpaired surrogates.
