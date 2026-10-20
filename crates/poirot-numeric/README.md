# poirot-numeric

Compare spreadsheet numbers and convert them to text.

Use `compare` for the implemented Excel-style fifteen-digit comparison rules.
`number_text` converts floating-point numbers to text, and `radix` provides
positional-number helpers. Behavior is based on Apache POI.

These operations are separate from cell display formatting, which lives in
[poirot-format](https://docs.rs/poirot-format).
