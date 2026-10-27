use super::super::candidate_index::CandidateIndex;
use super::super::execution::Execution;
use super::super::producer_candidate::ProducerCandidate;
use super::super::settlement_attempt::SettlementAttempt;
use super::super::settlement_slot::SettlementSlot;
use super::super::sheet_candidates::SheetCandidates;
use super::super::spill_access::SpillAccess;
use super::super::spill_demand::SpillDemand;
use super::super::spill_query::SpillQuery;
use crate::eval::limits::{BudgetMeter, EvaluationBudgets, EvaluationError};
use crate::eval::session::Session;
use crate::eval::values::{Area, AreaKind};
use crate::model::{CellAddress, Workbook};

#[test]
fn completed_component_reuses_maps_without_exposing_previous_attempt_state() {
    let mut slot = SettlementSlot::default();
    assert!(slot.active().is_none());
    slot.begin(Default::default(), 3, 5);
    let state = slot.active_mut().unwrap();
    let key = (0, CellAddress { row: 1, col: 2 });
    state.selected.insert(key);
    state.blocked.insert(key);
    state.admitted.insert(key);
    state.consumers.insert(key);
    state.random_touched.insert(key);
    state.non_repeatable = true;
    let pointer = state as *const SettlementAttempt;
    let capacity = state.admitted.capacity();
    slot.finish();
    assert!(slot.active().is_none());
    slot.begin(Default::default(), 7, 11);
    let state = slot.active().unwrap();
    assert_eq!(state as *const SettlementAttempt, pointer);
    assert_eq!(state.admitted.capacity(), capacity);
    assert!(state.selected.is_empty());
    assert!(state.blocked.is_empty());
    assert!(state.admitted.is_empty());
    assert!(state.consumers.is_empty());
    assert!(state.random_touched.is_empty());
    assert!(!state.non_repeatable);
    assert_eq!((state.journal_len, state.admitted_len), (7, 11));
}

#[test]
fn completed_range_proofs_are_sheet_specific_and_disabled_after_later_failure() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    book.add_sheet("T")
        .unwrap()
        .set_formula("Z1", "SEQUENCE(100)")
        .unwrap();
    let mut session = Session::with_context(&book, Default::default());
    let area = Area {
        kind: AreaKind::Area,
        sheet: 0,
        r0: 0,
        r1: 1,
        c0: 0,
        c1: 0,
    };
    SpillDemand::new(&mut session).resolve(area).unwrap();
    assert!(SpillQuery::new(&session).ready(area));
    assert!(!SpillQuery::new(&session).ready(Area { sheet: 1, ..area }));
    session.state.runtime.budget = BudgetMeter::new(EvaluationBudgets {
        materialized_values: 2,
        ..Default::default()
    })
    .unwrap();
    assert!(
        SpillDemand::new(&mut session)
            .resolve(Area {
                sheet: 1,
                r0: 9,
                r1: 9,
                c0: 25,
                c1: 25,
                ..area
            })
            .is_err()
    );
    assert!(session.state.runtime.preparation.failure.is_some());
    assert!(!SpillQuery::new(&session).ready(area));
    assert!(SpillDemand::new(&mut session).resolve(area).is_err());
    assert!(
        session
            .state
            .runtime
            .preparation
            .settlement
            .active()
            .is_none()
    );
}

#[test]
fn private_nested_ranges_never_publish_completed_area_proofs() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula("A1", "SEQUENCE(SUM(C1:C2))").unwrap();
    sheet.set_value("C1", 1.).unwrap();
    sheet.set_value("C2", 1.).unwrap();
    let mut session = Session::with_context(&book, Default::default());
    let area = Area {
        kind: AreaKind::Cell,
        sheet: 0,
        r0: 1,
        r1: 1,
        c0: 0,
        c1: 0,
    };
    SpillDemand::new(&mut session).resolve(area).unwrap();
    assert!(session.state.runtime.preparation.ready.is_none());
    SpillDemand::new(&mut session)
        .prepare(Area {
            kind: AreaKind::Area,
            ..area
        })
        .unwrap();
    assert!(session.state.runtime.preparation.ready.is_some());
    assert!(SpillQuery::new(&session).ready(area));
    session.state.runtime.preparation.failure = Some(EvaluationError::StalePlan);
    assert!(!SpillQuery::new(&session).ready(area));
    assert!(SpillDemand::new(&mut session).resolve(area).is_err());
}

#[test]
fn candidate_row_filter_matches_an_independent_rectangle_oracle() {
    let mut discovery = SheetCandidates::default();
    // Nested and unbounded-height members deliberately prevent perfect pruning.
    for (row, column, height, width) in [
        (0, 2, Some(1), Some(2)),
        (1, 0, Some(4), Some(1)),
        (2, 4, None, Some(1)),
        (4, 1, Some(1), None),
        (6, 3, Some(2), Some(2)),
    ] {
        let candidate = ProducerCandidate::new(
            0,
            CellAddress { row, col: column },
            crate::model::ShapeBounds {
                rows: height.and_then(std::num::NonZeroU32::new),
                columns: width.and_then(std::num::NonZeroU32::new),
            },
        );
        discovery.ending_rows.push(
            discovery
                .ending_rows
                .last()
                .copied()
                .unwrap_or(0)
                .max(candidate.area.r1),
        );
        discovery.candidates.push(candidate);
    }
    for r0 in 0..10 {
        for r1 in r0..10 {
            for c0 in 0..7 {
                for c1 in c0..7 {
                    let area = Area {
                        kind: AreaKind::Area,
                        sheet: 0,
                        r0,
                        r1,
                        c0,
                        c1,
                    };
                    for before in [
                        None,
                        Some(CellAddress { row: 4, col: 1 }),
                        Some(CellAddress { row: 0, col: 0 }),
                    ] {
                        let expected: Vec<_> = discovery
                            .candidates
                            .iter()
                            .filter(|c| {
                                before.is_none_or(|anchor| c.anchor < anchor)
                                    && c.anchor.row <= r1
                                    && c.area.r1 >= r0
                                    && c.anchor.col <= c1
                                    && c.area.c1 >= c0
                            })
                            .map(|c| c.anchor)
                            .collect();
                        let actual: Vec<_> = discovery
                            .possible_in(area, before)
                            .iter()
                            .filter(|c| c.intersects(area))
                            .map(|c| c.anchor)
                            .collect();
                        assert_eq!(actual, expected);
                    }
                }
            }
        }
    }
}

#[test]
fn interleaved_discovery_rebuilds_the_candidate_row_summary() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_formula(CellAddress { row: 8, col: 0 }, "SEQUENCE(2)")
        .unwrap();
    sheet
        .set_formula(CellAddress { row: 2, col: 4 }, "SEQUENCE(3)")
        .unwrap();
    let mut session = Session::with_context(&book, Default::default());
    SpillAccess::new(&mut session)
        .discover(0, CellAddress { row: 9, col: 0 })
        .unwrap();
    SpillAccess::new(&mut session)
        .discover(0, CellAddress { row: 4, col: 4 })
        .unwrap();
    SpillQuery::new(&session)
        .inspect(0, |p| {
            assert_eq!(
                p.candidates
                    .iter()
                    .map(|c| c.anchor.row)
                    .collect::<Vec<_>>(),
                [2, 8]
            );
            assert_eq!(p.ending_rows, [4, 9]);
        })
        .unwrap();
}

#[test]
fn frontier_edges_equality_and_retained_boundary_have_exact_strips() {
    let mut discovery = SheetCandidates::default();
    let mut budget = BudgetMeter::new(Default::default()).unwrap();
    let mut apply = |last: CellAddress| {
        let expansion = discovery
            .expansion(0, last, &mut budget, None)
            .unwrap()
            .unwrap();
        let strips = expansion.strips;
        for col in expansion.removed_columns {
            discovery.inspected.remove(&col);
        }
        discovery.inspected.insert(last.col, last.row);
        strips
    };
    let right = crate::model::MAX_COLUMNS - 1;
    let bottom = crate::model::MAX_ROWS - 1;
    let first = apply(CellAddress { row: 0, col: right });
    assert_eq!(first.len(), 1);
    let second = apply(CellAddress {
        row: bottom,
        col: 0,
    });
    assert_eq!(second.len(), 1);
    assert_eq!(
        (
            second[0].first.row,
            second[0].first.col,
            second[0].last.row,
            second[0].last.col
        ),
        (1, 0, bottom, 0)
    );
    let third = apply(CellAddress {
        row: bottom,
        col: right,
    });
    assert_eq!(third.len(), 1);
    assert_eq!(
        (
            third[0].first.row,
            third[0].first.col,
            third[0].last.row,
            third[0].last.col
        ),
        (1, 1, bottom, right)
    );
    assert_eq!(discovery.inspected.len(), 1);
    assert!(
        discovery
            .expansion(
                0,
                CellAddress {
                    row: bottom,
                    col: right
                },
                &mut budget,
                None
            )
            .unwrap()
            .is_none()
    );
    discovery.inspected = std::collections::BTreeMap::from([(2, 10), (5, 3)]);
    let expansion = discovery
        .expansion(0, CellAddress { row: 6, col: 4 }, &mut budget, None)
        .unwrap()
        .unwrap();
    assert!(expansion.removed_columns.is_empty());
    assert_eq!(expansion.strips.len(), 1);
    let strip = expansion.strips[0];
    assert_eq!(
        (
            strip.first.row,
            strip.first.col,
            strip.last.row,
            strip.last.col
        ),
        (4, 3, 6, 4)
    );
    let equal = discovery
        .expansion(0, CellAddress { row: 4, col: 5 }, &mut budget, None)
        .unwrap()
        .unwrap();
    assert!(equal.removed_columns.is_empty());
    assert_eq!(equal.frontier_size, 2);
}

#[test]
fn long_antidiagonal_frontier_collapses_without_revisiting_source_strips() {
    let mut discovery = SheetCandidates::default();
    let mut budget = BudgetMeter::new(Default::default()).unwrap();
    for row in 0..1024 {
        let last = CellAddress {
            row,
            col: 1023 - row,
        };
        let expansion = discovery
            .expansion(0, last, &mut budget, None)
            .unwrap()
            .unwrap();
        assert!(expansion.removed_columns.is_empty());
        assert_eq!(expansion.strips.len(), 1);
        assert_eq!(expansion.strips[0].first.row, row);
        discovery.inspected.insert(last.col, last.row);
    }
    let expansion = discovery
        .expansion(
            0,
            CellAddress {
                row: 1023,
                col: 1023,
            },
            &mut budget,
            None,
        )
        .unwrap()
        .unwrap();
    assert_eq!(expansion.removed_columns.len(), 1023);
    assert!(!expansion.removed_columns.contains(&1023));
    assert_eq!(expansion.frontier_size, 1);
    for col in expansion.removed_columns {
        discovery.inspected.remove(&col);
    }
    discovery.inspected.insert(1023, 1023);
    assert_eq!(discovery.inspected.len(), 1);
    assert!(discovery.covers(CellAddress {
        row: 1023,
        col: 1023
    }));
}

#[test]
fn derived_discovery_preserves_send_and_sync_storage_bounds() {
    fn assert_bounds<T: Send + Sync>() {}
    assert_bounds::<CandidateIndex>();
}

#[test]
fn legacy_owner_storage_counts_distinct_owners_rather_than_children() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    let anchor = CellAddress { row: 0, col: 0 };
    sheet.set_formula(anchor, "SEQUENCE(1)").unwrap();
    for row in 1..256 {
        sheet
            .set_spill_cached_value(
                CellAddress { row, col: 0 },
                anchor,
                "SEQUENCE(1)",
                1.0.into(),
            )
            .unwrap();
    }
    let mut session = Session::with_context(&book, Default::default());
    session.state.runtime.budget = BudgetMeter::new(EvaluationBudgets {
        materialized_values: 1,
        ..Default::default()
    })
    .unwrap();
    SpillAccess::new(&mut session).discover(0, anchor).unwrap();
    let discovery = session
        .state
        .runtime
        .preparation
        .candidates
        .get(&0)
        .unwrap();
    assert_eq!(discovery.legacy.as_ref(), &[anchor]);
    assert_eq!(discovery.candidates.len(), 1);
    assert_eq!(discovery.candidates[0].area.r1, crate::model::MAX_ROWS - 1);
}

#[test]
fn frontier_strips_match_independent_coordinate_union_without_revisits() {
    let points: Vec<_> = (0..4)
        .flat_map(|row| (0..4).map(move |col| CellAddress { row, col }))
        .collect();
    for a in &points {
        for b in &points {
            for c in &points {
                let mut discovery = SheetCandidates::default();
                let mut visited = std::collections::BTreeSet::new();
                let mut budget = BudgetMeter::new(Default::default()).unwrap();
                for last in [*a, *b, *c] {
                    let Some(expansion) = discovery.expansion(0, last, &mut budget, None).unwrap()
                    else {
                        continue;
                    };
                    for strip in expansion.strips {
                        for row in strip.first.row..=strip.last.row {
                            for col in strip.first.col..=strip.last.col {
                                assert!(
                                    visited.insert(CellAddress { row, col }),
                                    "source revisited"
                                );
                            }
                        }
                    }
                    for column in expansion.removed_columns {
                        discovery.inspected.remove(&column);
                    }
                    discovery.inspected.insert(last.col, last.row);
                    let rows: Vec<_> = discovery.inspected.values().copied().collect();
                    assert!(rows.windows(2).all(|w| w[0] > w[1]));
                }
                for p in &points {
                    let expected = [*a, *b, *c]
                        .iter()
                        .any(|end| end.row >= p.row && end.col >= p.col);
                    assert_eq!(visited.contains(p), expected);
                    assert_eq!(discovery.covers(*p), expected);
                }
            }
        }
    }
}

#[test]
fn refused_expansion_keeps_prior_shared_completeness_and_never_publishes_partial_sources() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value(CellAddress { row: 0, col: 0 }, 1.).unwrap();
    sheet
        .set_formula(CellAddress { row: 0, col: 2 }, "SEQUENCE(2)")
        .unwrap();
    let shared = std::cell::RefCell::new(CandidateIndex::new());
    let mut refused = Session::with_context(&book, Default::default());
    refused.reuse.spill_candidates = Some(&shared);
    refused.state.runtime.budget = BudgetMeter::new(EvaluationBudgets {
        materialized_values: 0,
        ..Default::default()
    })
    .unwrap();
    assert!(
        SpillAccess::new(&mut refused)
            .discover(0, CellAddress { row: 0, col: 0 })
            .is_err()
    );
    assert!(
        shared.borrow().is_empty(),
        "a failed first scan cannot publish an unbuilt legacy set"
    );
    let mut session = Session::with_context(&book, Default::default());
    session.reuse.spill_candidates = Some(&shared);
    let first = CellAddress { row: 1, col: 0 };
    SpillAccess::new(&mut session).discover(0, first).unwrap();
    let initial = shared.borrow().get(&0).unwrap().inspected.clone();
    session.state.runtime.budget = BudgetMeter::new(EvaluationBudgets {
        materialized_values: 1,
        ..Default::default()
    })
    .unwrap();
    assert!(
        SpillAccess::new(&mut session)
            .discover(0, CellAddress { row: 0, col: 2 })
            .is_err()
    );
    assert_eq!(shared.borrow().get(&0).unwrap().inspected, initial);
    assert!(shared.borrow().get(&0).unwrap().covers(first));
    assert!(
        !shared
            .borrow()
            .get(&0)
            .unwrap()
            .covers(CellAddress { row: 0, col: 2 })
    );
    let mut fresh = Session::with_context(&book, Default::default());
    fresh.reuse.spill_candidates = Some(&shared);
    SpillAccess::new(&mut fresh).discover(0, first).unwrap();
    assert_eq!(shared.borrow().get(&0).unwrap().inspected, initial);
    SpillAccess::new(&mut fresh)
        .discover(0, CellAddress { row: 0, col: 2 })
        .unwrap();
    let expanded = shared.borrow();
    let expanded = expanded.get(&0).unwrap();
    assert!(expanded.covers(first));
    assert_eq!(expanded.candidates.len(), 1);
    assert_eq!(
        expanded.candidates[0].anchor,
        CellAddress { row: 0, col: 2 }
    );
}
