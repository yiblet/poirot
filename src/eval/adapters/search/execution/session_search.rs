use super::plan::{LookupTable, LookupVector};
use super::search_axis::SearchAxis;
use super::search_mode::SearchMode;
use super::selection::select_with_reader;
use super::{prefix, sparse};
use crate::eval::host;
use crate::eval::limits::{EvalResult, EvaluationError};
use crate::eval::session::{CellEvaluator, Session};
use crate::eval::values::FunctionResult;
use crate::model::CellValue::Error;
use crate::model::{CellAddress, CellValue};

/// Reads lookup members and runs classic lookups through evaluator-owned storage.
pub(in crate::eval) struct SessionSearch<'s, 'a> {
    /// The run whose state this executor reads and updates.
    session: &'s mut Session<'a>,
}

impl<'s, 'a> SessionSearch<'s, 'a> {
    /// Borrow the session for this executor's calls.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Read one checked lookup-vector member through evaluator-owned storage.
    pub(in crate::eval) fn lookup_read(
        &mut self,
        table: &LookupTable,
        vector: LookupVector,
        index: usize,
    ) -> EvalResult<CellValue> {
        let (row, col) = match vector.coordinates(index) {
            Ok(v) => v,
            Err(e) => return Ok(Error(e)),
        };
        match table {
            LookupTable::SheetCells(areas) => {
                let a = *areas.get(row).ok_or(EvaluationError::StalePlan)?;
                CellEvaluator::new(self.session).range_cell(
                    a.sheet,
                    CellAddress {
                        row: a.r0,
                        col: a.c0,
                    },
                )
            }
            LookupTable::Reference(a) => CellEvaluator::new(self.session).range_cell(
                a.sheet,
                CellAddress {
                    row: a.r0 + row as u32,
                    col: a.c0 + col as u32,
                },
            ),
            LookupTable::Signed(a) => CellEvaluator::new(self.session).signed_range_read(
                a.read(row as u64, col as u64)
                    .ok_or(EvaluationError::StalePlan)?,
            ),
            LookupTable::Array(a) => {
                let value = a
                    .get(row)
                    .and_then(|values| values.get(col))
                    .ok_or(EvaluationError::StalePlan)?;
                if let Some(text) = value.text_ref() {
                    self.session
                        .state
                        .runtime
                        .budget
                        .text(text.storage_bytes())?;
                }
                Ok(value.clone())
            }
        }
    }

    /// Select a classic lookup position using the existing cache or checked reader.
    pub(in crate::eval) fn classic_lookup_index(
        &mut self,
        table: &LookupTable,
        axis: SearchAxis,
        key: &CellValue,
        mode: SearchMode,
        kind: host::ClassicLookupKind,
    ) -> FunctionResult<Option<usize>> {
        let vector = table.vector(axis, 0)?;
        let is_match = kind == host::ClassicLookupKind::Match;
        // Wildcard comparison is a predicate, not a total order suitable for bisection.
        let pattern_match = is_match
            && key
                .text_ref()
                .is_some_and(|text| text.units().any(|unit| matches!(unit, 0x2a | 0x3f)));
        let excel_match = is_match && !pattern_match && mode != SearchMode::Exact;

        if excel_match {
            poirot_functions::lookup::classic_match_index(
                vector.len,
                if mode == SearchMode::Ascending {
                    poirot_functions::lookup::ClassicMatchMode::Ascending
                } else {
                    poirot_functions::lookup::ClassicMatchMode::Descending
                },
                &mut sparse::LookupReader {
                    session: self.session,
                    table,
                    vector,
                    key,
                    mode,
                    is_match,
                    forward: None,
                },
            )
            .map_err(|failure| match failure {
                poirot_functions::lookup::MatchSelectionError::Reader(failure) => failure,
                poirot_functions::lookup::MatchSelectionError::InvalidPosition => {
                    EvaluationError::StalePlan.into()
                }
            })
        } else {
            prefix::select(self.session, table, vector, key, mode, is_match).unwrap_or_else(|| {
                select_with_reader(
                    vector.len,
                    mode,
                    !is_match,
                    &mut sparse::LookupReader {
                        session: self.session,
                        table,
                        vector,
                        key,
                        mode,
                        is_match,
                        forward: None,
                    },
                )
            })
        }
    }
}
