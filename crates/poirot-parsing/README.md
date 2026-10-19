# poirot-parsing

Workbook-independent parsers for formula syntax and spreadsheet text.

Use `formula::parse_formula` to build a formula AST. The formula module also
supports scanning, printing, and reference translation. Other modules parse
criteria, numbers, dates, and date/time inputs.

This crate handles syntax; resolving workbook references and calculating results
belong to the caller or [poirot-eval](../poirot-eval/README.md).
