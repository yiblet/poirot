use crate::eval::config::EvaluationInputs;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::EvaluationOutcome;
use crate::eval::ports::{DependencyReadSink, ReadEvent};
use crate::model::{CellAddress, Workbook};
use std::time::UNIX_EPOCH;

pub(super) const A1: CellAddress = CellAddress { row: 0, col: 0 };

pub(super) const B1: CellAddress = CellAddress { row: 0, col: 1 };

pub(super) const C1: CellAddress = CellAddress { row: 0, col: 2 };

pub(super) fn context() -> EvaluationInputs {
    EvaluationInputs {
        as_of_time: UNIX_EPOCH,
        randomness_seed: 17,
        localization: crate::eval::config::Localization {
            utc_offset_seconds: 0,
            date_order: crate::eval::config::DateOrder::MonthDayYear,
        },
    }
}

pub(super) fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
}

pub(super) fn target(address: CellAddress) -> EvaluationTarget {
    EvaluationTarget { sheet: 0, address }
}

pub(super) fn value<T: std::fmt::Debug>(outcome: EvaluationOutcome<T>) -> T {
    match outcome {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}

#[derive(Default)]
/// Collects attributed consumed-read events for shared run assertions.
pub(super) struct Reads(pub(super) Vec<ReadEvent>);

impl DependencyReadSink for Reads {
    fn read(&mut self, event: ReadEvent) {
        self.0.push(event);
    }
}
