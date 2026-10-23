//! Keep overlay edits and calculated results across repeated input changes.
//! Flush edits into the owned workbook while retaining reusable calculations.
use poirot::{EvaluationContext, Overlay, Workbook};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut workbook = poirot::model::Workbook::new();
    workbook
        .add_sheet("S")?
        .set_formula("B1".parse()?, "A1*2")?;
    let mut overlay = Overlay::with_workbook(workbook).build()?;
    let input = overlay.target("S!A1")?;
    let output = overlay.target("S!B1")?;
    let context = EvaluationContext::default();
    for input_value in [3., 4.] {
        overlay.set_value(input, input_value)?;
        let result = overlay.evaluation(&context)?.cell(output)?;
        overlay.flush()?;
        let mut warm = overlay.evaluation(&context)?;
        if warm.cell(output)? != result || warm.usage().formula_executions != 0 {
            return Err("flush failed to retain the calculated result".into());
        }
    }
    // Build an export copy with current results. flush() updates inputs in the
    // baseline; calculated formula results remain in the overlay until applied.
    let mut saved = overlay.workbook().clone();
    saved.apply(overlay.changes())?;
    let bytes = Workbook::from_model(saved).save_xlsx().to_buffer()?;
    println!(
        "Exported {} XLSX bytes; calculation state remains warm",
        bytes.len()
    );
    Ok(())
}
