//! Consumed bounded membership queries: geometry is immutable; reads remain host effects.
use super::comparison::lookup_compare;
use super::search_axis::SearchAxis;
use super::search_mode::SearchMode;
use super::{plan, selection};
use crate::eval::adapters::search::execution::session_search::SessionSearch;
use crate::eval::config::ArrayResultPolicy;
use crate::eval::limits::{EvaluationFailure, WorkKind};
use crate::eval::ports::DependencyRead;
use crate::eval::session::{ReadRecorder, Session};
use crate::eval::spill::SpillDemand;
use crate::eval::values::{Area, FunctionFailure, FunctionResult};
use crate::model::{CellAddress, CellValue};
use plan::{LookupTable, LookupVector};
use selection::CandidateReader;
use std::cmp::Ordering;

/// Maps a lookup vector to worksheet intervals for sparse membership queries.
/// Logical indices still count absent cells, so skipping storage gaps does not shift MATCH results.
#[derive(Clone, Copy)]
struct MembershipVector {
    area: Area,
    axis: SearchAxis,
}

impl MembershipVector {
    fn new(table: &LookupTable, vector: LookupVector) -> Option<Self> {
        let LookupTable::Reference(area) = table else {
            return None;
        };
        let mut area = *area;
        match vector.axis {
            SearchAxis::Vertical => {
                area.c0 += vector.fixed as u32;
                area.c1 = area.c0;
            }
            SearchAxis::Horizontal => {
                area.r0 += vector.fixed as u32;
                area.r1 = area.r0;
            }
        }
        Some(Self {
            area,
            axis: vector.axis,
        })
    }

    fn interval(self, first: usize, end: usize) -> Area {
        let mut area = self.area;
        match self.axis {
            SearchAxis::Vertical => {
                area.r0 += first as u32;
                area.r1 = self.area.r0 + end as u32 - 1;
            }
            SearchAxis::Horizontal => {
                area.c0 += first as u32;
                area.c1 = self.area.c0 + end as u32 - 1;
            }
        }
        area
    }

    fn index(self, address: CellAddress) -> usize {
        match self.axis {
            SearchAxis::Vertical => (address.row - self.area.r0) as usize,
            SearchAxis::Horizontal => (address.col - self.area.c0) as usize,
        }
    }
}

/// Budgeted lookup candidate reader recording only consumed membership intervals.
pub(super) struct LookupReader<'a, 'w> {
    /// Shared execution state and dependency read sink.
    pub(super) session: &'a mut Session<'w>,
    /// Validated source table.
    pub(super) table: &'a LookupTable,
    /// Candidate vector within the table.
    pub(super) vector: LookupVector,
    /// Already resolved lookup key.
    pub(super) key: &'a CellValue,
    /// Exact or approximate matching rule.
    pub(super) mode: SearchMode,
    /// Whether MATCH-specific coercion and error rules apply.
    pub(super) is_match: bool,
    // Exact selection advances monotonically. Retain its borrowed membership
    // cursor, not values, so the next candidate need not reopen the same range.
    /// Borrowed cursor reused while exact matching advances monotonically.
    pub(super) forward: Option<ForwardCursor<'w>>,
}

/// A retained sparse cursor for successive forward lookup queries.
/// It tracks work already charged so advancing the same traversal does not charge those counters again.
pub(super) struct ForwardCursor<'w> {
    first: usize,
    end: usize,
    cells: crate::eval::inputs::EffectiveStoredCells<'w>,
    charged: crate::model::query::QueryCounters,
}

impl LookupReader<'_, '_> {
    fn query(&mut self, first: usize, end: usize, reverse: bool) -> FunctionResult<Option<usize>> {
        if first >= end {
            return Ok(None);
        }
        let Some(vector) = MembershipVector::new(self.table, self.vector) else {
            return Ok(Some(if reverse { end - 1 } else { first }));
        };
        let first_index = first;
        let area = vector.interval(first, end);
        SpillDemand::new(self.session).resolve(area)?;
        let first = CellAddress {
            row: area.r0,
            col: area.c0,
        };
        let last = CellAddress {
            row: area.r1,
            col: area.c1,
        };
        let membership = crate::model::query::RangeMembership::new(area.sheet, first, last)
            .map_err(|_| EvaluationFailure::StalePlan)?;
        let view = crate::eval::inputs::EffectiveWorkbook::new(
            self.session.sources.workbook,
            self.session.sources.inputs,
        );
        let generated = self
            .session
            .state
            .runtime
            .preparation
            .admitted_cells()
            .filter(|(sheet, at)| *sheet == area.sheet && area.contains(*at))
            .map(|(_, at)| *at);
        let generated = if reverse {
            generated.max()
        } else {
            generated.min()
        };
        let (address, counters) =
            if self.mode == SearchMode::Exact && !reverse && generated.is_none() {
                if self
                    .forward
                    .as_ref()
                    .is_none_or(|cursor| first_index < cursor.first || end != cursor.end)
                {
                    self.forward = Some(ForwardCursor {
                        first: first_index,
                        end,
                        cells: view
                            .stored(membership)
                            .map_err(|_| EvaluationFailure::StalePlan)?,
                        charged: crate::model::query::QueryCounters::default(),
                    });
                }
                // SAFETY: The branch assigns forward = Some when absent and never clears it before borrowing the cursor.
                let cursor = self.forward.as_mut().expect("prepared exact cursor");
                cursor.first = first_index;
                let address = cursor.cells.find_map(|(address, _)| {
                    (vector.index(*address) >= first_index).then_some(*address)
                });
                let total = cursor.cells.counters();
                let counters = crate::model::query::QueryCounters {
                    row_seeks: total.row_seeks - cursor.charged.row_seeks,
                    column_seeks: total.column_seeks - cursor.charged.column_seeks,
                    cells_visited: total.cells_visited - cursor.charged.cells_visited,
                    key_lookups: total.key_lookups - cursor.charged.key_lookups,
                };
                cursor.charged = total;
                (address, counters)
            } else if reverse {
                let mut cells = view
                    .stored_reverse(membership)
                    .map_err(|_| EvaluationFailure::StalePlan)?;
                let address = cells.next().map(|(address, _)| *address);
                (address, cells.counters())
            } else {
                let mut cells = view
                    .stored(membership)
                    .map_err(|_| EvaluationFailure::StalePlan)?;
                let address = cells.next().map(|(address, _)| *address);
                (address, cells.counters())
            };
        for amount in [
            counters.row_seeks,
            counters.column_seeks,
            counters.cells_visited,
        ] {
            self.session
                .state
                .runtime
                .budget
                .charge(WorkKind::SparseScan, amount)?;
        }
        let address = match (address, generated) {
            (Some(authored), Some(generated)) => Some(if reverse {
                authored.max(generated)
            } else {
                authored.min(generated)
            }),
            (authored, generated) => authored.or(generated),
        };
        // Record only the interval whose absence affected selection. The unconsumed
        // suffix/prefix remains outside the guard, just like its formula providers.
        let (guard_first, guard_last) = if reverse {
            (address.unwrap_or(first), last)
        } else {
            (first, address.unwrap_or(last))
        };
        if address.is_none() || guard_first != guard_last {
            let first =
                if self.session.policies.array_result_policy == ArrayResultPolicy::DynamicSpill {
                    CellAddress { row: 0, col: 0 }
                } else {
                    guard_first
                };
            ReadRecorder::new(self.session).read(DependencyRead::SparseMembership {
                sheet: area.sheet,
                first,
                last: guard_last,
            });
        }
        Ok(address.map(|address| vector.index(address)))
    }
}

impl CandidateReader for LookupReader<'_, '_> {
    fn next(&mut self, first: usize, end: usize) -> FunctionResult<Option<usize>> {
        self.query(first, end, false)
    }

    fn previous(&mut self, end: usize) -> FunctionResult<Option<usize>> {
        self.query(0, end, true)
    }

    fn compare(&mut self, index: usize) -> FunctionResult<Option<Ordering>> {
        let value = SessionSearch::new(self.session).lookup_read(self.table, self.vector, index)?;
        lookup_compare(
            self.key,
            &value,
            self.mode,
            self.is_match,
            &mut self.session.state.runtime.budget,
        )
    }
}

impl poirot_functions::lookup::MatchCandidateReader for LookupReader<'_, '_> {
    type Error = FunctionFailure;

    fn next(&mut self, first: usize, end: usize) -> FunctionResult<Option<usize>> {
        CandidateReader::next(self, first, end)
    }

    fn previous(&mut self, end: usize) -> FunctionResult<Option<usize>> {
        CandidateReader::previous(self, end)
    }

    fn compare(&mut self, index: usize) -> FunctionResult<Option<Ordering>> {
        CandidateReader::compare(self, index)
    }
}
