# poirot-functions-text

Text calculations for spreadsheet functions.

Includes title casing with `proper`, Roman numerals with `roman`, numeric-text
operations, and substring planning. The caller evaluates arguments and controls
allocation limits.

`java_case` implements Unicode casing. Case-insensitive comparison uses
[poirot-text](https://docs.rs/poirot-text), which also defines text counting and
slicing rules.

`modern` provides lossless UTF-16 delimiter selection and rectangular split plans
for `TEXTBEFORE`, `TEXTAFTER` and `TEXTSPLIT`, plus strict text escaping for
`ARRAYTOTEXT` and `VALUETOTEXT`. Shape inspection does not allocate output;
callers admit text, span storage and work before execution. Case-insensitive
matching uses Unicode scalar lowercase equality and preserves isolated surrogate
units. Exact Excel locale/collation equivalence remains unverified. The excessive-occurrence check currently
counts UTF-16 units; native Excel Version 2 behavior for this bound still needs
verification. The evaluator's extraction delimiter-array interpretation is also
tracked as an open compatibility question.

`regex` uses the safe PCRE2 API for lookarounds, backreferences, Unicode matching,
extraction and occurrence-aware replacement. Caller admission precedes searches
and owned output allocations. Native backtracking, depth and heap limits remain
operational failures. The 8-bit backend requires valid Unicode and rejects active
`\C` escapes; isolated UTF-16 surrogates are not converted lossily. Native Excel's
exact PCRE2 version/options and uncommon replacement spellings remain unverified.
See [dependency provenance and notices](../../LICENSES/PCRE2.md).
