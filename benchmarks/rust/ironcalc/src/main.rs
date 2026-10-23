//! Time IronCalc's raw Model API, without UI undo history or workbook file I/O.
use benchmark_common::{Address, Case, Edit, Engine, Error, Input};
use ironcalc_base::{Model, cell::CellValue};

/// Independently populated IronCalc model for one timing round.
struct Adapter(Model<'static>);

/// Translate zero-based protocol coordinates to IronCalc's one-based coordinates.
fn coordinates(address: Address) -> Result<(i32, i32), Error> {
    Ok((
        i32::try_from(address.row)?
            .checked_add(1)
            .ok_or("row overflow")?,
        i32::try_from(address.col)?
            .checked_add(1)
            .ok_or("column overflow")?,
    ))
}

/// Populate a fresh raw model inside the common construction boundary.
fn build(case: &Case) -> Result<Adapter, Error> {
    let mut model = Model::new_empty("benchmark", "en", "UTC", "en")?;
    for _ in 1..case.sheet_count {
        model.new_sheet();
    }
    for cell in &case.cells {
        let (row, col) = coordinates(cell.address)?;
        let input = match &cell.input {
            Input::Number(value) => value.to_string(),
            Input::Text { text } => format!("'{text}"),
            Input::Formula(formula) => formula.clone(),
        };
        model.set_user_input(u32::try_from(cell.address.sheet)?, row, col, input)?;
    }
    Ok(Adapter(model))
}

impl Engine for Adapter {
    fn read(&mut self, outputs: &[Address]) -> Result<Vec<f64>, Error> {
        self.0.evaluate();
        self.read_unchanged(outputs)
    }
    fn read_unchanged(&mut self, outputs: &[Address]) -> Result<Vec<f64>, Error> {
        outputs
            .iter()
            .map(|&address| {
                let (row, col) = coordinates(address)?;
                match self
                    .0
                    .get_cell_value_by_index(u32::try_from(address.sheet)?, row, col)?
                {
                    CellValue::Number(value) => Ok(value),
                    other => Err(format!("expected numeric output, got {other:?}").into()),
                }
            })
            .collect()
    }
    fn edit(&mut self, edit: &Edit) -> Result<(), Error> {
        let (row, col) = coordinates(edit.address)?;
        let value = match &edit.value {
            Input::Number(value) => value.to_string(),
            Input::Text { text } => format!("'{text}"),
            Input::Formula(value) => value.clone(),
        };
        self.0
            .set_user_input(u32::try_from(edit.address.sheet)?, row, col, value)?;
        Ok(())
    }
}

/// Execute the shared timing and correctness protocol against the pinned engine.
fn main() -> Result<(), Error> {
    benchmark_common::run("ironcalc", "0.9.0", "model", build)
}
