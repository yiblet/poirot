//! Compare quote quantities without changing the original workbook.
use poirot::{Evaluation, EvaluationContext, Workbook};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Quote")?;
    sheet.set_value("A1".parse()?, 12.)?;
    sheet.set_value("B1".parse()?, 25.)?;
    sheet.set_formula("C1".parse()?, "A1*B1")?;

    let mut evaluation = Evaluation::with_workbook(&book).build(EvaluationContext::default())?;
    let quantity = evaluation.target("Quote!A1")?;
    let total = evaluation.target("Quote!C1")?;

    for count in [12., 24.] {
        evaluation.set_value(quantity, count)?;
        println!("Quantity: {count}, total: {:?}", evaluation.cell(total)?);
    }
    Ok(())
}
