use super::key::Key;
use super::ordered_causes::OrderedCauses;
use super::validity::Validity;
use super::value_read_order::ValueReadOrder;
use crate::model::CellAddress;
use crate::model::query::RangeMembership;
use std::collections::BTreeMap;

/// Reads and execution properties that determine when one cached result expires.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(in crate::eval) struct Dependencies {
    /// Individual consumed cells, including absent cells.
    pub cells: Vec<Key>,
    /// Membership guards that also detect newly inserted cells in scanned ranges.
    pub ranges: Vec<RangeMembership>,
    /// Consecutive point reads compressed into rectangles.
    pub point_runs: Vec<RangeMembership>,
    /// Actual value reads in execution order, compressed into forward point spans.
    /// Repeated spans are harmless: validation selects each cause's first occurrence.
    pub value_reads: ValueReadOrder,
    /// Metadata reads cannot be certified by comparing cell values alone.
    pub metadata_reads: bool,
    /// Geometry-only guards; literal value changes do not create new producers.
    pub spill_ranges: Vec<RangeMembership>,
    /// Execution consumed a provider whose dependencies cannot be fully observed.
    pub opaque: bool,
    /// Execution must be repeated in each new evaluation.
    pub volatile: bool,
    /// Execution context fields consumed while computing the result.
    pub context: crate::eval::config::ContextMask,
    /// Whether formatting configuration affected the result.
    pub formatting: bool,
    /// Numeric input locale was consumed.
    #[serde(default)]
    pub number_locale: bool,
    /// Whether the result owns a spill region or belongs to another cell's spill.
    pub spill: super::spill_role::SpillRole,
    /// A possible producer needs ownership validation even when its current result is scalar.
    pub spill_sensitive: bool,
}

impl Dependencies {
    pub(super) fn value_read(&mut self, (sheet, at): Key) {
        if let Some(last) = self.value_reads.last_mut() {
            if last.contains(sheet, at) {
                return;
            }
            if last.sheet == sheet
                && last.first.col == last.last.col
                && at.col == last.last.col
                && Some(at.row) == last.last.row.checked_add(1)
            {
                last.last = at;
                return;
            }
            if last.sheet == sheet
                && last.first.row == last.last.row
                && at.row == last.last.row
                && Some(at.col) == last.last.col.checked_add(1)
            {
                last.last = at;
                return;
            }
        }
        self.value_reads.push(RangeMembership {
            sheet,
            first: at,
            last: at,
        });
    }

    /// Order dirty value dependencies by their first consumed read; other evidence forces execution.
    pub fn ordered_causes(
        &self,
        validity: &Validity,
        budget: &mut crate::eval::limits::BudgetMeter,
    ) -> crate::eval::limits::EvalResult<Option<OrderedCauses>> {
        if self.metadata_reads {
            return Ok(None);
        }
        match validity {
            // A single cause needs no ordering evidence. Cached lookup indexes,
            // for example, can retain membership without replaying point reads.
            Validity::Cause { key, version } => {
                Ok(Some(OrderedCauses::One(Some((*key, *version)))))
            }
            Validity::Causes(causes) => {
                if matches!(self.value_reads, ValueReadOrder::Unavailable) {
                    return Ok(None);
                }
                if let ValueReadOrder::One(span) = &self.value_reads {
                    if (span.first.col != span.last.col && span.first.row != span.last.row)
                        || span.first.row > span.last.row
                        || span.first.col > span.last.col
                    {
                        return Ok(None);
                    }
                    // Establish complete evidence before evaluating anything: an
                    // unordered membership cause could change an old lazy branch.
                    for key in causes.keys() {
                        budget.charge(crate::eval::limits::WorkKind::Comparison, 1)?;
                        if !span.contains(key.0, key.1) {
                            return Ok(None);
                        }
                    }
                    return Ok(Some(OrderedCauses::Span {
                        next: Some(span.first),
                        span: *span,
                        remaining: causes.len(),
                    }));
                }
                // Irregular multi-span traces need a complete safe ordering.
                // Index only dirty causes, not the potentially enormous scanned ranges.
                let mut rows = BTreeMap::new();
                let mut columns = BTreeMap::new();
                for (&key, &version) in causes {
                    budget.charge(crate::eval::limits::WorkKind::Comparison, 1)?;
                    rows.insert((key.0, key.1.row, key.1.col), version);
                    columns.insert((key.0, key.1.col, key.1.row), version);
                }
                let mut ordered = Vec::new();
                if ordered.try_reserve(causes.len()).is_err() {
                    return Ok(None);
                }
                for span in self.value_reads.iter() {
                    budget.charge(crate::eval::limits::WorkKind::Comparison, 1)?;
                    let vertical = span.first.col == span.last.col;
                    if !vertical && span.first.row != span.last.row {
                        return Ok(None);
                    }
                    let (first, last) = if vertical {
                        (
                            (span.sheet, span.first.col, span.first.row),
                            (span.sheet, span.last.col, span.last.row),
                        )
                    } else {
                        (
                            (span.sheet, span.first.row, span.first.col),
                            (span.sheet, span.last.row, span.last.col),
                        )
                    };
                    if first > last {
                        return Ok(None);
                    }
                    loop {
                        let index = if vertical { &columns } else { &rows };
                        // SAFETY: The local comparison above proves the inclusive range is ordered.
                        let next = index.range(first..=last).next().map(
                            |(&(sheet, axis, offset), &version)| {
                                let address = if vertical {
                                    CellAddress {
                                        row: offset,
                                        col: axis,
                                    }
                                } else {
                                    CellAddress {
                                        row: axis,
                                        col: offset,
                                    }
                                };
                                ((sheet, address), version)
                            },
                        );
                        let Some((key, version)) = next else {
                            break;
                        };
                        rows.remove(&(key.0, key.1.row, key.1.col));
                        columns.remove(&(key.0, key.1.col, key.1.row));
                        // SAFETY: Reserved causes.len() slots above; removing each selected key
                        // from both indexes means at most that many unique causes are pushed.
                        ordered.push((key, version));
                    }
                    if rows.is_empty() {
                        return Ok(Some(OrderedCauses::Many(ordered.into_iter())));
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    pub(super) fn cell(&mut self, (sheet, at): Key) {
        // Membership already invalidates every cell in this rectangle, including
        // missing cells. Keep outside reads such as spill owners as point edges.
        if self
            .ranges
            .last()
            .is_some_and(|range| range.contains(sheet, at))
        {
            return;
        }
        if let Some(run) = self.point_runs.last_mut() {
            if run.contains(sheet, at) {
                return;
            }
            if run.sheet == sheet
                && run.first.col == run.last.col
                && at.col == run.last.col
                && at.row == run.last.row + 1
            {
                run.last = at;
                return;
            }
            if run.sheet == sheet
                && run.first.row == run.last.row
                && at.row == run.last.row
                && at.col == run.last.col + 1
            {
                run.last = at;
                return;
            }
        }
        if let Some(&(previous_sheet, previous)) = self.cells.last() {
            if previous_sheet == sheet
                && ((previous.col == at.col && previous.row + 1 == at.row)
                    || (previous.row == at.row && previous.col + 1 == at.col))
            {
                self.cells.pop();
                self.point_runs.push(RangeMembership {
                    sheet,
                    first: previous,
                    last: at,
                });
                return;
            }
            if (previous_sheet, previous) == (sheet, at) {
                return;
            }
        }
        self.cells.push((sheet, at));
    }
}
