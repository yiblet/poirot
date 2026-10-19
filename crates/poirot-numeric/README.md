# poirot-numeric

Shared floating-point comparison and number-to-text primitives with behavior
based on Apache POI.

Use `compare` for the supported Excel-style fifteen-digit comparison rules,
`number_text` for POI-compatible numeric text conversion, and `radix` for
positional-number helpers. These operations are separate from cell display
formatting, which lives in [poirot-format](../poirot-format/README.md).
