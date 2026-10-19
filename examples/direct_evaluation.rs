//! Editable evaluation over a shared baseline without dependency-graph bookkeeping.
use poirot::{CellValue, Evaluation, EvaluationContext, Workbook};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("S")?
        .set_formula("B1".parse()?, "A1*2", CellValue::Blank)?;
    let workbook = Arc::new(workbook.into_model());
    let mut evaluation =
        Evaluation::with_workbook(Arc::clone(&workbook)).build(EvaluationContext::default())?;
    let input = evaluation.target("S!A1")?;
    let output = evaluation.target("S!B1")?;
    evaluation.set_value(input, 3.)?;
    if evaluation.cell(output)? != CellValue::Number(6.) {
        return Err("unexpected evaluation result".into());
    }
    evaluation.set_value(input, 4.)?;
    if evaluation.cell(output)? != CellValue::Number(8.) {
        return Err("unexpected evaluation result".into());
    }
    let bytes = evaluation.save_snapshot().to_buffer()?;
    let restored = Workbook::load_snapshot().from_bytes(&bytes)?;
    let mut restored = restored
        .into_evaluation(
            EvaluationContext::default(),
            poirot::OverlayPolicy::default(),
        )?
        .evaluation;
    if restored.cell(output)? != CellValue::Number(8.) {
        return Err("unexpected evaluation result".into());
    }
    if restored.compiled_usage().parse_attempts != 0 {
        return Err("restoration unexpectedly parsed formulas".into());
    }
    Ok(())
}
