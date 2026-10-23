//! Compare energy use for different operating hours without changing the workbook.
//! A1 holds operating hours, B1 power in kW, and C1 energy use in kWh.
//! Evaluation retains input edits and compiled formulas; each edit clears results.
use poirot::Workbook;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Energy")?;
    sheet.set_value("A1".parse()?, 12.)?;
    sheet.set_value("B1".parse()?, 25.)?;
    sheet.set_formula("C1".parse()?, "A1*B1")?;

    let mut evaluation = book.evaluation().build()?;
    let hours = evaluation.target("Energy!A1")?;
    let energy = evaluation.target("Energy!C1")?;

    for hours_per_day in [12., 24.] {
        evaluation.set_value(hours, hours_per_day)?;
        println!(
            "Hours per day: {hours_per_day}, energy (kWh): {:?}",
            evaluation.cell(energy)?
        );
    }
    Ok(())
}
