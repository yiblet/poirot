# poirot-formula

Parse Excel formula expressions without loading a workbook or running an
evaluator. The syntax tree preserves references, operators, missing arguments
and function spellings. Name resolution and function availability belong to the
consumer.

```rust
use poirot_formula::{FormulaPrinter, parse_formula};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let expression = parse_formula("SUM(A1:A3)*2")?;
    let printed = FormulaPrinter::print(&expression)?;
    assert_eq!(parse_formula(&printed)?, expression);
    Ok(())
}
```

`parse_formula_with_policy` adds explicit parsing limits and source metadata.
`TranslationPlan` prepares a bounded reference translation between
`FormulaLocation`s. Printing changed syntax is canonical; it is not an original
source-text round trip. Use `poirot-workbook::edit` and `translation_edit` to apply
source-checked formula transactions to a workbook.

Parsing does not establish that a formula is supported by an evaluator or XLS
writer. Those consumers retain their own admission rules and operational limits.

To inspect exact function spellings and argument counts, retain source metadata:

```rust
use poirot_formula::{parse_formula_with_policy, FormulaParsePolicy};
let parsed = parse_formula_with_policy("SUM(A1:A3)", FormulaParsePolicy::default())?;
let call = &parsed.function_invocations()[0];
assert_eq!(&parsed.source()[call.name_span.start..call.name_span.end], "SUM");
assert_eq!(call.argument_count, 1);
# Ok::<(), poirot_formula::ParseError>(())
```
