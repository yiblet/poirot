//! Edit, evaluate, apply owned changes, and save without a separate calculator.
use poirot::{CellValue, EvaluationContext, Overlay, Workbook, eval};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Inputs")?;
    sheet.set_value("A1".parse()?, 21.)?;
    sheet.set_formula("B1".parse()?, "A1*2", CellValue::Blank)?;
    let target = eval::EvaluationTarget {
        sheet: 0,
        address: "B1".parse()?,
    };
    let context = EvaluationContext::default();
    let changes = {
        let mut overlay = Overlay::with_workbook(&book).build()?;
        overlay.set_value(
            eval::EvaluationTarget {
                sheet: 0,
                address: "A1".parse()?,
            },
            24.,
        )?;
        let mut evaluation = overlay.evaluation(&context)?;
        if evaluation.cell(target)? != CellValue::Number(48.) {
            return Err("unexpected evaluation result".into());
        }
        drop(evaluation);
        overlay.changes()
    };
    book.apply(changes)?;
    for bytes in [book.save_xlsx().to_buffer()?, book.save_xls().to_buffer()?] {
        let reopened = Workbook::from_bytes(&bytes)?;
        let saved = reopened
            .sheet(0)
            .and_then(|sheet| sheet.get(target.address))
            .ok_or("saved formula cell is missing")?;
        if saved.value != CellValue::Number(48.) {
            return Err("unexpected saved value".into());
        }
    }
    Ok(())
}
