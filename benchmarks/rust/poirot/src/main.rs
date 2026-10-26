//! Measure direct scenario and incremental Poirot evaluation using the common protocol.
use benchmark_common::{Address, Case, Edit, Engine, Error, Input};
// Engine scenarios own their models and expose execution reports for benchmark diagnostics.
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::{CellAddress, CellValue};
use poirot::calculation::{EvaluationContext, EvaluationTarget};

/// A native scenario mode selected once outside measurement.
#[derive(Clone, Copy)]
enum Mode {
    Evaluation,
    Overlay,
    EvaluationBatch,
    OverlayBatch,
    WorkbookCell,
}

/// Result publication boundary used by one adapter workflow.
#[derive(Clone, Copy)]
enum OutputMode {
    Cell,
    Batch,
}

/// Owned workbook and calculation state for one fresh-model round.
enum Model {
    /// Direct workbook-cell evaluation with fresh execution state for each output.
    WorkbookCell(Box<poirot::model::Workbook>),
    /// Direct scenario execution, clearing results on edits.
    Evaluation(
        Box<Evaluation<'static, poirot::model::Workbook>>,
        OutputMode,
    ),
    /// Incremental execution retaining unaffected results across edits.
    Overlay(Box<Overlay<'static, poirot::model::Workbook>>, OutputMode),
}

/// Convert shared zero-based coordinates without string parsing.
fn target(address: Address) -> Result<EvaluationTarget, Error> {
    Ok(EvaluationTarget {
        sheet: address.sheet,
        address: CellAddress::new(address.row, address.col)?,
    })
}

/// Reject spreadsheet errors and nonnumeric outputs rather than timing wrong answers.
fn number(value: CellValue) -> Result<f64, Error> {
    match value {
        CellValue::Number(value) => Ok(value),
        other => Err(format!("expected numeric output, got {other:?}").into()),
    }
}

/// Populate a fresh workbook and select native calculation state inside the build timer.
fn build(case: &Case, mode: Mode) -> Result<Model, Error> {
    let mut book = poirot::model::Workbook::new();
    for index in 0..case.sheet_count {
        // SAFETY: index is below sheet_count, so it cannot equal usize::MAX.
        book.add_sheet(format!("Sheet{}", index + 1))?;
    }
    for cell in &case.cells {
        let address = CellAddress::new(cell.address.row, cell.address.col)?;
        let sheet = book
            .sheet_mut(cell.address.sheet)
            .ok_or("missing source sheet")?;
        match &cell.input {
            Input::Number(value) => sheet.set_value(address, *value)?,
            Input::Text { text } => sheet.set_value(address, text.as_str())?,
            Input::Formula(formula) => sheet.set_formula(address, formula)?,
        }
    }
    Ok(match mode {
        Mode::WorkbookCell => Model::WorkbookCell(Box::new(book)),
        Mode::Evaluation | Mode::EvaluationBatch => Model::Evaluation(
            Box::new(Evaluation::with_workbook(book).build(context())?),
            if matches!(mode, Mode::EvaluationBatch) {
                OutputMode::Batch
            } else {
                OutputMode::Cell
            },
        ),
        Mode::Overlay | Mode::OverlayBatch => Model::Overlay(
            Box::new(Overlay::with_workbook(book).build()?),
            if matches!(mode, Mode::OverlayBatch) {
                OutputMode::Batch
            } else {
                OutputMode::Cell
            },
        ),
    })
}

/// Fixed context prevents changing the clock from invalidating reusable work.
fn context() -> EvaluationContext {
    EvaluationContext {
        as_of_time: std::time::UNIX_EPOCH,
        randomness_seed: 42,
        ..Default::default()
    }
}

impl Engine for Model {
    fn read(&mut self, outputs: &[Address]) -> Result<Vec<f64>, Error> {
        match self {
            Self::WorkbookCell(book) => outputs
                .iter()
                .map(|&a| {
                    let a = target(a)?;
                    let context = context();
                    let configuration = poirot::eval::RunConfiguration::from(context.inputs());
                    match poirot::eval::run_cell_with_configuration(
                        book,
                        a.sheet,
                        a.address,
                        &configuration,
                        context.limits.into(),
                    ) {
                        poirot::eval::EvaluationOutcome::Complete { value, .. } => number(value),
                        poirot::eval::EvaluationOutcome::Aborted { failure: error, .. } => {
                            Err(format!("{error:?}").into())
                        }
                    }
                })
                .collect(),
            Self::Evaluation(model, OutputMode::Batch) => model
                .outputs(
                    &outputs
                        .iter()
                        .copied()
                        .map(target)
                        .collect::<Result<Vec<_>, _>>()?,
                )?
                .into_iter()
                .map(|result| number(result.value))
                .collect(),
            Self::Evaluation(model, OutputMode::Cell) => outputs
                .iter()
                .map(|&a| number(model.cell(target(a)?)?))
                .collect(),
            Self::Overlay(model, mode) => {
                let context = context();
                let mut run = model.evaluation(&context)?;
                if matches!(mode, OutputMode::Batch) {
                    return run
                        .outputs(
                            &outputs
                                .iter()
                                .copied()
                                .map(target)
                                .collect::<Result<Vec<_>, _>>()?,
                        )?
                        .into_iter()
                        .map(|result| number(result.value))
                        .collect();
                }
                outputs
                    .iter()
                    .map(|&a| number(run.cell(target(a)?)?))
                    .collect()
            }
        }
    }
    fn retains_results(&self) -> bool {
        !matches!(self, Self::WorkbookCell(_))
    }
    fn edit(&mut self, edit: &Edit) -> Result<(), Error> {
        let address = target(edit.address)?;
        match self {
            Self::WorkbookCell(book) => {
                let sheet = book.sheet_mut(address.sheet).ok_or("missing edit sheet")?;
                match &edit.value {
                    Input::Number(value) => sheet.set_value(address.address, *value)?,
                    Input::Text { text } => sheet.set_value(address.address, text.as_str())?,
                    Input::Formula(value) => sheet.set_formula(address.address, value)?,
                }
            }
            Self::Evaluation(model, _) => match &edit.value {
                Input::Number(value) => model.set_value(address, *value)?,
                Input::Text { text } => model.set_value(address, text.as_str())?,
                Input::Formula(value) => model.set_formula(address, value)?,
            },
            Self::Overlay(model, _) => match &edit.value {
                Input::Number(value) => model.set_value(address, *value)?,
                Input::Text { text } => model.set_value(address, text.as_str())?,
                Input::Formula(value) => model.set_formula(address, value)?,
            },
        }
        Ok(())
    }
}

/// Select the requested mode and execute the shared benchmark protocol.
fn main() -> Result<(), Error> {
    let name = std::env::args_os()
        .nth(1)
        .unwrap_or_else(|| "overlay".into())
        .into_string()
        .map_err(|_| "mode must be valid UTF-8")?;
    let mode = match name.as_str() {
        "evaluation" => Mode::Evaluation,
        "overlay" => Mode::Overlay,
        "evaluation-batch" => Mode::EvaluationBatch,
        "overlay-batch" => Mode::OverlayBatch,
        "workbook-cell" => Mode::WorkbookCell,
        _ => {
            return Err(
                "mode must be evaluation, overlay, evaluation-batch, overlay-batch, or workbook-cell".into(),
            );
        }
    };
    benchmark_common::run("poirot", env!("CARGO_PKG_VERSION"), &name, |case| {
        build(case, mode)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_workflows_preserve_order_duplicates_and_edits() -> Result<(), Error> {
        let case: Case = serde_json::from_str(
            r#"{
            "name":"paired", "cells":[{"row":0,"col":0,"input":2},
            {"row":0,"col":1,"input":"=A1*3"},
            {"row":1,"col":1,"input":"=A1+5"}],
            "outputs":[{"row":1,"col":1},{"row":0,"col":1},{"row":1,"col":1}],
            "expected":[7,6,7], "edits":[{"row":0,"col":0,"value":4,"expected":[9,12,9]}]
        }"#,
        )?;
        for mode in [
            Mode::Evaluation,
            Mode::EvaluationBatch,
            Mode::Overlay,
            Mode::OverlayBatch,
        ] {
            let mut model = build(&case, mode)?;
            assert_eq!(model.read(&case.outputs)?, case.expected);
            for edit in &case.edits {
                model.edit(edit)?;
                assert_eq!(model.read(&case.outputs)?, edit.expected);
            }
            if let Model::Evaluation(model, _) = model {
                // Each of two runs executes both formulas; duplicate outputs use the memo.
                assert_eq!(model.finish().execution.formula_executions, 4);
            }
        }
        Ok(())
    }

    #[test]
    fn stateless_and_retained_routes_support_cross_sheet_source_and_literal_edits()
    -> Result<(), Error> {
        let case: Case = serde_json::from_str(
            r#"{
            "name":"cross_sheet_literal", "sheet_count":2,
            "cells":[{"sheet":1,"row":0,"col":0,"input":{"text":"=A1"}},
                     {"row":0,"col":1,"input":"=LEN(Sheet2!A1)*2"}],
            "outputs":[{"row":0,"col":1}], "expected":[6],
            "edits":[{"row":0,"col":1,"value":"=LEN(Sheet2!A1)*3","expected":[9]},
                     {"sheet":1,"row":0,"col":0,"value":{"text":"plain"},"expected":[15]}]
        }"#,
        )?;
        for mode in [
            Mode::Evaluation,
            Mode::Overlay,
            Mode::EvaluationBatch,
            Mode::OverlayBatch,
            Mode::WorkbookCell,
        ] {
            let mut model = build(&case, mode)?;
            assert_eq!(model.read(&case.outputs)?, case.expected);
            assert_eq!(model.read_unchanged(&case.outputs)?, case.expected);
            assert_eq!(model.retains_results(), !matches!(mode, Mode::WorkbookCell));
            for edit in &case.edits {
                model.edit(edit)?;
                assert_eq!(model.read(&case.outputs)?, edit.expected);
            }
        }
        Ok(())
    }

    #[test]
    #[ignore = "requires BENCHMARK_SUITE; prints counters separately from timed runs"]
    fn scaling_execution_counters() -> Result<(), Error> {
        let suite: benchmark_common::Suite =
            serde_json::from_reader(std::fs::File::open(std::env::var("BENCHMARK_SUITE")?)?)?;
        for case in suite.cases {
            for mode in [Mode::Evaluation, Mode::EvaluationBatch] {
                let mut model = build(&case, mode)?;
                benchmark_common::validate(
                    &model.read(&case.outputs)?,
                    &case.expected,
                    &case.name,
                )?;
                if let Model::Evaluation(model, output_mode) = model {
                    let name = if matches!(output_mode, OutputMode::Batch) {
                        "batch"
                    } else {
                        "cell"
                    };
                    println!("{} {name}: {:?}", case.name, model.finish());
                }
            }
        }
        Ok(())
    }
}
