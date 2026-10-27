# poirot-parsing

Parse spreadsheet formulas and text values independently of workbook storage.
Use `formula::parse_formula` to build an expression tree; references remain
unresolved until a caller supplies workbook context. Parsing does not evaluate
formulas. Applications calculating workbooks should start with
[poirot::calculation](https://docs.rs/poirot/latest/poirot/calculation/).

```rust
fn main() -> Result<(), poirot_parsing::formula::ParseError> {
    use poirot_parsing::formula::{parse_formula, Expr};
    let expression = parse_formula("=42")?;
    assert!(matches!(expression, Expr::Number(n) if n == 42.0));
    Ok(())
}
```

Formula failures expose a byte `position`, a typed `ParseReasonError` in
`ParseError::reason`, and a broad `kind()` classification. The reason is also the
standard error source; render the error chain for a complete diagnostic.
`NumberLocale::new` returns `Result<NumberLocale, NumberLocaleError>` so callers
can inspect invalid locale tokens without parsing diagnostic text.

| Module | Purpose |
| --- | --- |
| `formula` | Formula AST, printing and checked reference translation |
| `criteria` | Comparison prefixes and source-specific criterion operand syntax |
| `number` | Numeric input grammar with explicit separators and affixes |
| `date` | Borrowed DATEVALUE/TIMEVALUE date fields |
| `datetime` | Source-specific optional datetime syntax and unresolved fields |
| `formatter` | Pinned formatter datetime grammar with committed optional sections |
| `java_number` | Java hexadecimal floating literal grammar and binary64 rounding |

The text parsers expose particular spreadsheet/source grammars, rather than one
universal date or number parser. Numeric input uses an explicit `NumberLocale`
and never consults the process locale. Date/time fields still require caller
calendar validation and default-year resolution. Use
[poirot-functions](https://docs.rs/poirot-functions) for spreadsheet coercion and
calendar algorithms built on these syntax modules.
