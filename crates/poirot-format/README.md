# poirot-format

Parse Excel number-format strings and render cell values as display text,
including numeric, date, time, and conditional sections.

Use `compile_format` to prepare a format and `render_value` to apply it to a
`CellValue`. Formatting changes the displayed text, not the stored value.
Compilation and rendering return diagnostics for unsupported syntax, fidelity
limits, and resource failures.
