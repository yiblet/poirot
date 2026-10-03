//! Shared fixtures for the run contract tests.
use super::*;
use crate::{CompilerUsage, EvaluationPolicy, FormulaCompiler, RegistryIdentity};
use poirot_model::{CellError, CellValue};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
const A1: CellAddress = CellAddress { row: 0, col: 0 };
const B1: CellAddress = CellAddress { row: 0, col: 1 };
const C1: CellAddress = CellAddress { row: 0, col: 2 };
fn context() -> EvaluationInputs {
    EvaluationInputs {
        as_of_time: UNIX_EPOCH,
        randomness_seed: 17,
        localization: crate::Localization {
            utc_offset_seconds: 0,
            date_order: crate::DateOrder::MonthDayYear,
        },
    }
}
fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
}
fn target(address: CellAddress) -> EvaluationTarget {
    EvaluationTarget { sheet: 0, address }
}
fn value<T: std::fmt::Debug>(outcome: EvaluationOutcome<T>) -> T {
    match outcome {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
#[derive(Default)]
/// Collects attributed consumed-read events for shared run assertions.
struct Reads(Vec<ReadEvent>);
impl DependencyReadSink for Reads {
    fn read(&mut self, event: ReadEvent) {
        self.0.push(event);
    }
}

mod admission_preparation;
mod cache_completion;
mod consumed_dependencies;
mod execution_configuration;
