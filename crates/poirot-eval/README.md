# poirot-eval

Compile and evaluate spreadsheet formulas.

Use `Evaluation` when you need calculated outputs without changing the original
workbook. For example, try several input values and compare the results:

```rust
use poirot_eval::{overlay::Evaluation, EvaluationContext};
use poirot_model::{CellValue, Workbook};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut book = Workbook::new();
    book.add_sheet("Inputs")?.set_formula("B1".parse()?, "A1*2")?;
    let mut evaluation = Evaluation::with_workbook(&book)
        .build(EvaluationContext::default())?;
    let input = evaluation.target("Inputs!A1")?;
    let output = evaluation.target("Inputs!B1")?;
    for (input_value, expected) in [(21., 42.), (24., 48.)] {
        evaluation.set_value(input, input_value)?;
        assert_eq!(evaluation.cell(output)?, CellValue::Number(expected));
    }
    Ok(())
}
```

Creating an evaluation does not calculate anything. `cell` calculates one output
and its dependencies; `outputs` accepts a list, and `all` calculates every occupied
cell and any formula spill cells. Each input edit clears calculated results.

For repeated scenarios, `Overlay` tracks dependencies and keeps unaffected results
between calculations. Its `evaluation` method starts a calculation with a supplied
`EvaluationContext`. The context controls time, randomness, locale, and limits.

Use the compiler, function registry, and custom-function interfaces when embedding
the evaluator in your own engine. For ordinary workbook editing and file handling,
start with [poirot](https://docs.rs/poirot).
