# poirot-format

Parse Excel number formats and render typed cell values under explicit locale,
date-system and width policy. Formatting is separate from formula evaluation:
it produces display text without changing the underlying value.

`FormatPlan` compiles a reusable format. `render_byte_bound` supplies a conservative
output bound for callers that must admit allocation before rendering. Inspect
`FormatError::kind` to distinguish invalid syntax, unsupported fidelity and
resource refusal.

Typography, arbitrary locale calendars and DBNum numbering are unsupported.
Width-dependent `*` fill requires a character width. System F800/F400 selectors
use configured US English patterns, described in [system formats](docs/system-formats.md).
[Calendar policy](docs/calendar-policy.md) covers date-system boundaries.
Use `poirot::format` through the facade or depend on this crate directly.

For a single display, parse and render directly. Use the source workbook's date
system when interpreting numeric date serials.

```rust
use poirot_format::{parse_format, format_value, FormatOptions};
use poirot_model::{CellValue, DateSystem};
let format = parse_format("0.00")?;
let displayed = format_value(&CellValue::Number(12.5), &format, FormatOptions {
    date_system: DateSystem::Excel1900,
    width: None,
})?;
assert_eq!(displayed.text, "12.50");
# Ok::<(), poirot_format::FormatError>(())
```
