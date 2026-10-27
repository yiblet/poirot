//! Bounded typed exact-search prefixes and text selections retained by incremental overlays.
//! Observed runs keep ordinary reads; retained axes expire when scanned cells change.
use super::comparison::lookup_compare;
use super::search_axis::SearchAxis;
use super::search_mode::SearchMode;
use super::{plan, selection, sparse};
use crate::eval::adapters::search::execution::session_search::SessionSearch;
use crate::eval::config::ArrayResultPolicy;
use crate::eval::limits::{EvaluationError, WorkKind};
use crate::eval::ports::DependencyRead;
use crate::eval::reuse::{MAX_ENTRIES, PrefixAxis, PrefixKey, SearchPrefix};
use crate::eval::session::{ReadRecorder, Session};
use crate::eval::spill::{SpillDemand, SpillQuery};
use crate::eval::values::{Area, FunctionResult};
use crate::model::CellValue::{Error, Number};
use crate::model::{CellAddress, CellValue};
use plan::{LookupTable, LookupVector};
use selection::CandidateReader;
use std::cmp::Ordering;

/// None means fall back to the ordinary selector; Some carries an actual result.
pub(super) fn select(
    session: &mut Session<'_>,
    table: &LookupTable,
    vector: LookupVector,
    key: &CellValue,
    mode: SearchMode,
    is_match: bool,
) -> Option<FunctionResult<Option<usize>>> {
    if !session.state.runtime.lookup_prefixes.enabled
        || mode != SearchMode::Exact
        || session.ports.dependency_sink.is_some()
        || session.ports.cell_reader.is_some()
        || session.ports.cell_observer.is_some()
        || session.ports.builtin_invocation_observer.is_some()
        || session.ports.execution_observer.is_some()
        || session.sources.extensions.is_some()
    {
        return None;
    }
    let needle = PrefixKey::search(key);
    let text = key.text_ref();
    if needle.is_none() && text.is_none() {
        return None;
    }
    let LookupTable::Reference(area) = table else {
        return None;
    };
    let (first, last) = match vector.axis {
        SearchAxis::Vertical => (
            CellAddress {
                row: area.r0,
                col: area.c0 + vector.fixed as u32,
            },
            CellAddress {
                row: area.r1,
                col: area.c0 + vector.fixed as u32,
            },
        ),
        SearchAxis::Horizontal => (
            CellAddress {
                row: area.r0 + vector.fixed as u32,
                col: area.c0,
            },
            CellAddress {
                row: area.r0 + vector.fixed as u32,
                col: area.c1,
            },
        ),
    };
    let axis = PrefixAxis {
        sheet: area.sheet,
        first,
        last,
    };
    let search_area = Area {
        sheet: axis.sheet,
        r0: first.row,
        r1: last.row,
        c0: first.col,
        c1: last.col,
        ..*area
    };
    if let Err(failure) = SpillDemand::new(session).resolve(search_area) {
        return Some(Err(failure.into()));
    }
    if session.policies.array_result_policy == ArrayResultPolicy::DynamicSpill
        && !SpillQuery::new(session).ready(search_area)
    {
        return None;
    }

    let cap = MAX_ENTRIES.min(session.state.runtime.budget.limits.materialized_values);
    if session.state.runtime.lookup_prefixes.active.contains(&axis)
        || (session.state.runtime.lookup_prefixes.reserved >= cap
            && !session.state.runtime.lookup_prefixes.contains(axis))
    {
        return None;
    }
    // Bound query storage before cloning. One owned unit reserves two bytes;
    // together with entry/cell counts this also bounds total retained text bytes.
    let text = if let Some(text) = text {
        let units = text.units().count();
        if units > cap {
            return None;
        }
        let mut query = Vec::new();
        if query.try_reserve_exact(units).is_err() {
            return None;
        }
        // SAFETY: units counts this immutable text iterator; the reservation
        // above covers every code unit copied by extend.
        query.extend(text.units());
        Some(query)
    } else {
        None
    };
    let existing = session.state.runtime.lookup_prefixes.remove(axis);
    let mut prefix = match existing {
        Some(prefix) => prefix,
        None => {
            if let Err(failure) = session.state.runtime.budget.allocation(1) {
                return Some(Err(failure.into()));
            }
            session.state.runtime.lookup_prefixes.reserved += 1;
            SearchPrefix::default()
        }
    };
    session.state.runtime.lookup_prefixes.active.insert(axis);
    let mut retain = true;
    let result = (|| {
        session
            .state
            .runtime
            .budget
            .charge(WorkKind::Comparison, 1)?;
        if let Some(found) = text.as_ref().and_then(|query| prefix.text.get(query)) {
            if session.state.runtime.budget.profiling {
                session.state.runtime.run_usage.lookup_prefix_hits = session
                    .state
                    .runtime
                    .run_usage
                    .lookup_prefix_hits
                    .saturating_add(1);
            }
            return Ok(*found);
        }
        if let Some(needle) = needle {
            if let Some(&found) = prefix.first.get(&needle) {
                if session.state.runtime.budget.profiling {
                    session.state.runtime.run_usage.lookup_prefix_hits = session
                        .state
                        .runtime
                        .run_usage
                        .lookup_prefix_hits
                        .saturating_add(1);
                }
                return Ok(Some(found));
            }
            if prefix.exhausted {
                if session.state.runtime.budget.profiling {
                    session.state.runtime.run_usage.lookup_prefix_hits = session
                        .state
                        .runtime
                        .run_usage
                        .lookup_prefix_hits
                        .saturating_add(1);
                }
                return Ok(None);
            }
        }
        let mut reader = sparse::LookupReader {
            session,
            table,
            vector,
            key,
            mode,
            is_match,
            forward: None,
        };
        let mut next = reader.next(if text.is_some() { 0 } else { prefix.next }, vector.len)?;
        while let Some(index) = next {
            if reader.session.state.runtime.budget.profiling {
                reader.session.state.runtime.run_usage.lookup_prefix_scans = reader
                    .session
                    .state
                    .runtime
                    .run_usage
                    .lookup_prefix_scans
                    .saturating_add(1);
            }
            let value = SessionSearch::new(reader.session).lookup_read(table, vector, index)?;
            // A recursive provider can temporarily yield #REF before completing.
            // Do not retain a prefix that skipped any spreadsheet error/nonfinite.
            if matches!(value, Error(_)) || matches!(value,Number(n) if !n.is_finite()) {
                retain = false;
            }
            if let Some(candidate) = PrefixKey::candidate(&value)
                && let std::collections::hash_map::Entry::Vacant(entry) =
                    prefix.first.entry(candidate)
            {
                if reader.session.state.runtime.lookup_prefixes.reserved < cap {
                    reader.session.state.runtime.budget.allocation(1)?;
                    reader.session.state.runtime.lookup_prefixes.reserved += 1;
                    entry.insert(index);
                } else {
                    retain = false;
                }
            }
            let following = index.checked_add(1).ok_or(EvaluationError::StalePlan)?;
            prefix.next = prefix.next.max(following);
            if lookup_compare(
                key,
                &value,
                mode,
                is_match,
                &mut reader.session.state.runtime.budget,
            )? == Some(Ordering::Equal)
            {
                return Ok(Some(index));
            }
            next = reader.next(following, vector.len)?;
        }
        prefix.exhausted = true;
        Ok(None)
    })();
    if retain && let (Some(query), Ok(selected)) = (text, &result) {
        let requested = query.len().saturating_add(1);
        if !prefix.text.contains_key(&query)
            && requested <= cap.saturating_sub(session.state.runtime.lookup_prefixes.reserved)
            && prefix.text.try_reserve(1).is_ok()
        {
            // SAFETY: requested is positive, and the local remaining-cap check
            // proves reserved + requested <= cap <= usize::MAX.
            session.state.runtime.lookup_prefixes.reserved += requested;
            prefix.text_storage = prefix.text_storage.saturating_add(requested);
            prefix.text.insert(query, *selected);
        }
    }
    // A reused prefix carries selection values from other consumers. Attribute
    // its consumed prefix to this consumer too, so any changed candidate dirties
    // every lookup that reused it. Session sources cannot change during execution.
    if session.reuse.overlay_reads.is_some()
        && let Ok(selected) = &result
    {
        let last = selected.map_or(axis.last, |index| match vector.axis {
            SearchAxis::Vertical => CellAddress {
                row: axis.first.row + index as u32,
                col: axis.first.col,
            },
            SearchAxis::Horizontal => CellAddress {
                row: axis.first.row,
                col: axis.first.col + index as u32,
            },
        });
        let first = if session.policies.array_result_policy == ArrayResultPolicy::DynamicSpill {
            CellAddress { row: 0, col: 0 }
        } else {
            axis.first
        };
        ReadRecorder::new(session).read(DependencyRead::SparseMembership {
            sheet: axis.sheet,
            first,
            last,
        });
    }
    session.state.runtime.lookup_prefixes.active.remove(&axis);
    if retain && result.is_ok() {
        session.state.runtime.lookup_prefixes.insert(axis, prefix);
    }
    // Dropped/failed prefixes keep their admission reservation. This bounds all
    // nested allocations conservatively without a second unbounded retention map.
    Some(result)
}

#[cfg(test)]
mod interval_tests {
    use super::*;
    use crate::eval::compile;
    use crate::eval::config::EvaluationInputs;
    use crate::eval::coordinates::EvaluationTarget;
    use crate::eval::limits::EvalResult;
    use crate::eval::reuse::LookupPrefixes;
    use crate::eval::session::ResultProjection;
    use crate::model::CellError::{NA, Num};
    use crate::model::Workbook;

    fn lookup(run: &mut Session<'_>, formula: &str) -> EvalResult<CellValue> {
        let parsed = poirot_parsing::formula::parse_formula(formula).unwrap();
        let bound = compile::bound::FormulaPlan::bind(&parsed).unwrap();
        let at = "Z20".parse().unwrap();
        let value = crate::eval::vm::ExpressionRunner::new(run).expr(0, at, &bound.expression())?;
        ResultProjection::new(run).select_legacy_scalar(at, value)
    }

    #[test]
    fn error_nonfinite_and_capacity_failures_discard_extended_prefixes() {
        for suffix in [Error(NA), Number(f64::INFINITY), Number(2.)] {
            let mut book = Workbook::new();
            let sheet = book.add_sheet("S").unwrap();
            sheet.set_value("A1", 1.).unwrap();
            sheet.set_value("A2", suffix.clone()).unwrap();
            sheet.set_value("A3", 3.).unwrap();
            let mut run = Session::with_context(&book, EvaluationInputs::default());
            run.policies.array_result_policy = ArrayResultPolicy::ScalarIntersection;
            run.state.runtime.lookup_prefixes.enable();
            assert_eq!(lookup(&mut run, "MATCH(1,A1:A3,0)").unwrap(), Number(1.));
            assert_eq!(run.state.runtime.lookup_prefixes.axes.len(), 1);
            if suffix == Number(2.) {
                run.state.runtime.budget.limits.materialized_values = 3;
            }
            // POI numeric ingress retains infinity; comparison rejects it as NUM.
            let expected = if matches!(suffix, Number(n) if !n.is_finite()) {
                Error(Num)
            } else {
                Number(3.)
            };
            assert_eq!(lookup(&mut run, "MATCH(3,A1:A3,0)").unwrap(), expected);
            assert!(
                run.state.runtime.lookup_prefixes.axes.is_empty(),
                "{suffix:?}"
            );
        }
    }

    #[test]
    fn budget_error_discards_the_extended_prefix() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet.set_value("A1", 1.).unwrap();
        sheet.set_value("A2", "a long unread literal").unwrap();
        let mut run = Session::with_context(&book, EvaluationInputs::default());
        run.state.runtime.lookup_prefixes.enable();
        assert_eq!(lookup(&mut run, "MATCH(1,A1:A2,0)").unwrap(), Number(1.));
        // The retained first candidate succeeds; extending to the next candidate
        // refuses its text before the newly extended prefix can be published.
        run.state.runtime.budget.limits.text_bytes = 8;
        let failure = lookup(&mut run, "MATCH(2,A1:A2,0)").unwrap_err();
        assert_eq!(
            failure.budget().unwrap().target,
            Some(EvaluationTarget {
                sheet: 0,
                address: "A2".parse().unwrap()
            })
        );
        assert!(run.state.runtime.lookup_prefixes.axes.is_empty());
    }

    #[test]
    fn interval_guards_cover_present_cells_gaps_and_overlapping_axes() {
        let vertical = PrefixAxis {
            sheet: 0,
            first: CellAddress { row: 2, col: 4 },
            last: CellAddress { row: 9, col: 4 },
        };
        let horizontal = PrefixAxis {
            sheet: 0,
            first: CellAddress { row: 4, col: 2 },
            last: CellAddress { row: 4, col: 9 },
        };
        for row in 0..12 {
            for col in 0..12 {
                for exhausted in [false, true] {
                    let mut prefixes = LookupPrefixes::default();
                    for axis in [vertical, horizontal] {
                        prefixes.insert(
                            axis,
                            SearchPrefix {
                                next: 4,
                                exhausted,
                                ..Default::default()
                            },
                        );
                    }
                    let changed = CellAddress { row, col };
                    prefixes.invalidate(&[(0, changed)].into_iter().collect());
                    for axis in [vertical, horizontal] {
                        let in_range = row >= axis.first.row
                            && row <= axis.last.row
                            && col >= axis.first.col
                            && col <= axis.last.col;
                        let offset = if axis.first.col == axis.last.col {
                            row.saturating_sub(axis.first.row)
                        } else {
                            col.saturating_sub(axis.first.col)
                        };
                        assert_eq!(
                            prefixes.contains(axis),
                            !(in_range && (exhausted || offset < 4))
                        );
                    }
                }
            }
        }
    }
}
