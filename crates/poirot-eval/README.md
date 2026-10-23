# poirot-eval

Compile and evaluate spreadsheet models with direct or incremental calculation.

Use `run_cell` or `run_formula` for a direct calculation against workbook data.
Use `Evaluation` for editable scenarios and `Overlay` to retain dependency
information and reuse results across input changes.

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

Edits stay in the evaluation or overlay until explicitly applied. An overlay also
retains intermediate results; changed inputs, context settings, or volatile
functions invalidate affected results. Native snapshots can save state for later
restoration.

Register custom functions through `extensions` and attach the registry with the
builder's `with_extensions` method. Registrations can add application-specific
functions but cannot override existing builtins.

Use the compiler, function registry, and custom-function interfaces when embedding
the evaluator in your own engine. For ordinary workbook editing and file handling,
start with [poirot](https://docs.rs/poirot).
