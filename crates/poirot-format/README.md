# poirot-format

Render cell values with Excel number formats.

Compile a format once with `compile_format`, then use `render_value` for each
value. Formats can contain numeric, date, time, and text sections. Rendering
changes the displayed text, not the stored cell value.

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use poirot_format::{compile_format, render_value, FormatCompilePolicy, FormatRenderContext};
    use poirot_model::CellValue;

    let format = compile_format("0.00", FormatCompilePolicy::default())?;
    let displayed = render_value(
        &CellValue::Number(12.5), &format, FormatRenderContext::default(),
    )?;
    assert_eq!(displayed.text, "12.50");
    Ok(())
}
```

Unsupported formats return errors. Date rendering uses the supplied workbook
calendar; locale and width are explicit rendering options.
