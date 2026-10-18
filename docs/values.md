# Values, text and formatting

[Documentation](README.md)

Poirot keeps cell types distinct. A blank, numeric zero, empty text and a boolean
are different values. `CellValue::Error` represents a spreadsheet error; Rust
errors report operations that could not complete.

## Blanks and argument conversion

Reading or selecting a value preserves its type. The consuming function or
operator decides whether conversion is allowed. There is no universal rule that
turns all numeric-looking text, booleans or blanks into numbers.

For example, selecting an empty result cell from a successful lookup yields a
blank inside the expression. An aggregate can ignore that selected blank,
arithmetic can use it as zero, concatenation can use it as empty text, and
`ISBLANK` can inspect it. A missing lookup match remains `#N/A`. Direct scalar
arguments, referenced cells and array members can have different admission rules.

Completed worksheet formulas publish a remaining blank as numeric zero, including
spill members, before memoization and downstream reads. Physical empty cells
remain blank and empty text remains text. Ad hoc expressions and direct function
calls do not publish worksheet cells and can retain blank results.

## Numeric text and locale

Set `EvaluationContext::number_locale` with a validated `NumberLocaleV2` to
configure literal decimal, grouping, currency, percent and time tokens. The
default is explicit US-English input syntax, independent of process locale and
display currency settings.

Consumers choose which input forms they admit. A number-input locale does not
make text into numeric storage or broaden every function's grammar. Builtin
compatibility policies can also select different date or numeric parsing rules.
`localization` supplies date ordering and a fixed UTC offset; the workbook supplies
its date system. Keep these inputs explicit for reproducible calculations.

## Exact text

`CellValue::Text(String)` holds UTF-8 text. `CellValue::TextUtf16(ExcelText)` also
preserves exact UTF-16 units, including isolated surrogates. Exhaustive matches
must handle both variants. Use `text_ref()` and its unit view for unit-based
operations; request a fallible UTF-8 projection when a consumer requires Unicode
scalar values. Exact text identity compares UTF-16 units across representations.

Unicode scalar operations can refuse isolated surrogates operationally rather
than replace them. That failure also escapes `IFERROR`. Unit-based operations
can preserve those units successfully.

`OverlayPolicy::text` selects counting, slicing and case semantics. Under
`ExcelCompatibilityV2Documented`, LEN/MID/LEFT/RIGHT/REPLACE count adjacent valid
surrogate pairs once; isolated surrogates, combining marks and variation selectors
remain independent. FIND/SEARCH retain pinned UTF-16 indexing. Unicode case
conversion has explicit contextual limits and does not claim complete Excel
casing parity. See the [text policy contract](../crates/poirot-text/CONTRACT.md).

## Display a value

A number format changes presentation, not the numeric value used in calculation.
`outputs()` returns optional source-cell formatting, including overlay style
edits. Use the workbook's date system when rendering date serials. `TEXT()`
produces a text value as part of the formula itself.

```rust
use poirot::{CellValue, DateSystem};
use poirot::format::{FormatOptions, format_value, parse_format};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let format = parse_format("0.00")?;
    let displayed = format_value(&CellValue::Number(12.5), &format, FormatOptions {
        date_system: DateSystem::Excel1900,
        width: None,
    })?;
    println!("{}", displayed.text); // 12.50
    Ok(())
}
```

Use `FormatPlan` for reusable formats. `FormatError::kind` distinguishes invalid
syntax, unsupported fidelity and resource limits. See the
[formatting guide](../crates/poirot-format/README.md) for width and calendar limits.
