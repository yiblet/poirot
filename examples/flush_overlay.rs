//! Export repeated scenarios without committing edits or discarding warm calculations.
use poirot::{EvaluationContext, Overlay, Workbook};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut book = Workbook::new();
    book.add_sheet("S")?.set_formula("B1".parse()?, "A1*2")?;
    // Owned advanced construction permits explicitly applying authored edits.
    let mut overlay =
        Overlay::from_overlay(poirot::eval::overlay::Overlay::with_workbook(book).build()?);
    let input = overlay.target("S!A1")?;
    let output = overlay.target("S!B1")?;
    let context = EvaluationContext::default();
    for value in [3., 4.] {
        overlay.set_value(input, value)?;
        overlay.evaluation(&context)?.cell(output)?;
        // Applies only authored edits. Calculated results remain warm in the overlay.
        overlay.apply_edits_to_workbook()?;
    }
    let bytes = overlay.save_xlsx().to_buffer()?;
    println!(
        "Exported {} XLSX bytes; edits and calculations remain available",
        bytes.len()
    );
    Ok(())
}
