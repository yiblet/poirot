//! Sparse member reads: retained sources, overrides, scan budgets and order.

use super::super::item_folder::ItemFolder;
use super::super::sparse_reader::SparseReader;
use crate::eval::config::{ArrayResultPolicy, EvaluationInputs, RunConfiguration};
use crate::eval::entry::{run_formula, run_formula_with_configuration};
use crate::eval::limits::{
    BudgetError, BudgetKind, BudgetMeter, EvaluationBudgets, EvaluationError, EvaluationOutcome,
};
use crate::eval::session;
use crate::eval::session::cells::CellEvaluator;
use crate::eval::session::composition::Session;
use crate::eval::values::{Area, AreaKind, Value};
use crate::model::CellError::{Div0, NA};
use crate::model::CellValue::{Error, Number};
use crate::model::{CellAddress, CellValue, MAX_ROWS, Workbook};

fn book() -> Workbook {
    let mut book = Workbook::new();
    book.add_sheet("Data").unwrap();
    book
}

fn area(r0: u32, r1: u32, c0: u32, c1: u32) -> Area {
    Area {
        kind: AreaKind::Area,
        sheet: 0,
        r0,
        r1,
        c0,
        c1,
    }
}

fn session(book: &Workbook, limits: EvaluationBudgets) -> Session<'_> {
    Session::new(
        book,
        EvaluationInputs::default(),
        BudgetMeter::new(limits).unwrap(),
    )
}

fn evaluate(book: &Workbook, formula: &str) -> EvaluationOutcome<CellValue> {
    run_formula(
        book,
        0,
        CellAddress { row: 20, col: 20 },
        formula,
        EvaluationInputs::default(),
        EvaluationBudgets {
            run_work: 500,
            cell_work: 500,
            ..EvaluationBudgets::default()
        },
    )
}

#[test]
fn retained_sparse_sources_remove_one_resolution_per_authored_member() {
    let mut book = book();
    let count = 4096u32;
    for row in 0..count {
        book.sheet_mut(0)
            .unwrap()
            .set_value(CellAddress { row, col: 0 }, f64::from(row + 1))
            .unwrap();
    }
    let mut direct = session(&book, EvaluationBudgets::default());
    direct.policies.array_result_policy = ArrayResultPolicy::ScalarIntersection;
    let addresses = SparseReader::new(&mut direct)
        .sparse_addresses(area(0, count - 1, 0, 0))
        .unwrap();
    session::SOURCE_RESOLUTIONS.with(|counter| counter.set(0));
    let ordinary: f64 = addresses
        .into_iter()
        .map(|address| {
            match CellEvaluator::new(&mut direct)
                .range_cell(0, address)
                .unwrap()
            {
                Number(value) => value,
                _ => panic!("expected authored numeric literal"),
            }
        })
        .sum();
    let old_count = session::SOURCE_RESOLUTIONS.with(|counter| counter.get());
    session::SOURCE_RESOLUTIONS.with(|counter| counter.set(0));
    let projected = ItemFolder::new(&mut direct)
        .items(Value::Refs(vec![area(0, count - 1, 0, 0)].into()))
        .unwrap();
    let retained_count = session::SOURCE_RESOLUTIONS.with(|counter| counter.get());
    let resolved: f64 = projected
        .into_iter()
        .map(|(value, indirect)| {
            assert!(indirect);
            match value {
                Number(value) => value,
                _ => panic!("expected authored numeric literal"),
            }
        })
        .sum();
    let expected = f64::from(count) * f64::from(count + 1) / 2.0;
    assert_eq!(ordinary, expected);
    assert_eq!(resolved, expected);
    assert_eq!(old_count, count as usize);
    assert_eq!(retained_count, 0);
}

#[test]
fn retained_members_use_overrides_and_deletions_without_source_resolution() {
    let mut book = book();
    for row in 0..3 {
        book.sheet_mut(0)
            .unwrap()
            .set_value(CellAddress { row, col: 0 }, 1.0)
            .unwrap();
    }
    let mut inputs = crate::eval::inputs::RunInputs::new(
        &book,
        [
            (0, CellAddress { row: 0, col: 0 }, Number(5.0)),
            (0, CellAddress { row: 3, col: 0 }, Number(7.0)),
        ],
        2,
        std::sync::Arc::new(()),
    )
    .unwrap();
    inputs
        .sheets
        .entry(0)
        .or_default()
        .set(
            CellAddress { row: 1, col: 0 },
            crate::eval::inputs::CellEdit::Delete,
        )
        .unwrap();
    let mut direct = session(&book, EvaluationBudgets::default());
    direct.sources.inputs = Some(&inputs);
    direct.policies.array_result_policy = ArrayResultPolicy::ScalarIntersection;
    session::SOURCE_RESOLUTIONS.with(|counter| counter.set(0));
    assert_eq!(
        ItemFolder::new(&mut direct)
            .items(Value::Refs(vec![area(0, 3, 0, 0)].into()))
            .unwrap(),
        vec![
            (Number(5.0), true),
            (Number(1.0), true),
            (Number(7.0), true)
        ]
    );
    assert_eq!(session::SOURCE_RESOLUTIONS.with(|counter| counter.get()), 0);
}

#[test]
fn generated_spill_members_keep_ordinary_source_resolution() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(CellAddress { row: 0, col: 0 }, "SEQUENCE(3)")
        .unwrap();
    let mut direct = session(&book, EvaluationBudgets::default());
    let members = SparseReader::new(&mut direct)
        .sparse_members(area(0, 2, 0, 0))
        .unwrap();
    assert_eq!(members.len(), 3);
    assert!(members[0].cell.is_some());
    assert!(members[1..].iter().all(|member| member.cell.is_none()));
    session::SOURCE_RESOLUTIONS.with(|counter| counter.set(0));
    assert_eq!(
        ItemFolder::new(&mut direct)
            .items(Value::Refs(vec![area(0, 2, 0, 0)].into()))
            .unwrap(),
        vec![
            (Number(1.0), true),
            (Number(2.0), true),
            (Number(3.0), true)
        ]
    );
    assert_eq!(session::SOURCE_RESOLUTIONS.with(|counter| counter.get()), 2);
}

#[test]
fn unprofiled_sparse_collection_still_refuses_a_finite_materialization_limit() {
    let mut book = book();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_value("A1", 7.0).unwrap();
    sheet.set_value("A2", 9.0).unwrap();
    let range = area(0, 2, 0, 0);
    let mut ordinary = Session::new(
        &book,
        EvaluationInputs::default(),
        BudgetMeter::configured(EvaluationBudgets::default(), false).unwrap(),
    );
    ordinary.policies.array_result_policy = ArrayResultPolicy::ScalarIntersection;
    assert_eq!(
        SparseReader::new(&mut ordinary)
            .sparse_addresses(range)
            .unwrap(),
        ["A1".parse::<CellAddress>().unwrap(), "A2".parse().unwrap()],
    );
    let mut limited = Session::new(
        &book,
        EvaluationInputs::default(),
        BudgetMeter::configured(
            EvaluationBudgets {
                materialized_values: 1,
                ..Default::default()
            },
            false,
        )
        .unwrap(),
    );
    limited.policies.array_result_policy = ArrayResultPolicy::ScalarIntersection;
    assert!(matches!(
        SparseReader::new(&mut limited).sparse_addresses(range),
        Err(EvaluationError::Budget(BudgetError {
            kind: BudgetKind::Materialization,
            limit: 1,
            consumed: 0,
            requested: 2,
            ..
        }))
    ));
}

#[test]
fn tiny_range_budget_ignores_far_away_stored_cells() {
    let mut book = book();
    let sheet = book.sheet_mut(0).unwrap();
    sheet
        .set_value(CellAddress { row: 1, col: 0 }, 7.0)
        .unwrap();
    for row in 1000..3000 {
        sheet
            .set_value(CellAddress { row, col: 500 }, 99.0)
            .unwrap();
    }
    // Isolate sparse traversal from the separately metered complete spill phase.
    let result = run_formula_with_configuration(
        &book,
        0,
        CellAddress { row: 20, col: 20 },
        "SUM(A1:A3)",
        &RunConfiguration::default()
            .with_array_result_policy(ArrayResultPolicy::ScalarIntersection),
        EvaluationBudgets {
            run_work: 500,
            cell_work: 500,
            ..Default::default()
        },
    );
    assert!(matches!(
        result,
        EvaluationOutcome::Complete {
            value: Number(7.0),
            ..
        }
    ));
    // One column-index seek, one page-map seek, and one stored slot visit.
    // Exhausting the loaded page performs no further tree lookup.
    assert_eq!(result.usage().sparse_entries, 3);
}

#[test]
fn empty_query_charges_terminal_seek_and_honors_limit() {
    let book = book();
    let mut session = session(&book, EvaluationBudgets::default());
    assert!(
        SparseReader::new(&mut session)
            .sparse_addresses(area(0, 9, 0, 0))
            .unwrap()
            .is_empty()
    );
    assert_eq!(session.state.runtime.budget.usage.sparse_entries, 1);
    let mut limited = session_for_empty(&book);
    assert!(matches!(
        SparseReader::new(&mut limited).sparse_addresses(area(0, 9, 0, 0)),
        Err(EvaluationError::Budget(BudgetError {
            kind: BudgetKind::RunWork,
            limit: 0,
            consumed: 0,
            requested: 1,
            target: None,
        }))
    ));
}

fn session_for_empty(book: &Workbook) -> Session<'_> {
    session(
        book,
        EvaluationBudgets {
            run_work: 0,
            ..EvaluationBudgets::default()
        },
    )
}

#[test]
fn row_major_order_overlaps_and_allocation_limit_are_preserved() {
    let mut book = book();
    for (row, col) in [(2, 0), (0, 2), (0, 0), (1, 1)] {
        book.sheet_mut(0)
            .unwrap()
            .set_value(CellAddress { row, col }, 1.0)
            .unwrap();
    }
    let mut session = session(&book, EvaluationBudgets::default());
    assert_eq!(
        SparseReader::new(&mut session)
            .sparse_addresses(area(0, 2, 0, 2))
            .unwrap(),
        vec![
            CellAddress { row: 0, col: 0 },
            CellAddress { row: 0, col: 2 },
            CellAddress { row: 1, col: 1 },
            CellAddress { row: 2, col: 0 }
        ]
    );
    assert!(matches!(
        evaluate(&book, "SUM((A1:C3,A1:C3))"),
        EvaluationOutcome::Complete {
            value: Number(8.0),
            ..
        }
    ));
    let mut limited = Session::new(
        &book,
        EvaluationInputs::default(),
        BudgetMeter::new(EvaluationBudgets {
            materialized_values: 3,
            ..EvaluationBudgets::default()
        })
        .unwrap(),
    );
    assert!(matches!(
        SparseReader::new(&mut limited).sparse_addresses(area(0, 2, 0, 2)),
        Err(EvaluationError::Budget(BudgetError {
            kind: BudgetKind::Materialization,
            ..
        }))
    ));
}

#[test]
fn absent_range_membership_reads_fresh_inserted_and_removed_cells() {
    let mut book = book();
    let query = "SUM(A1:A10)";
    assert!(matches!(
        evaluate(&book, query),
        EvaluationOutcome::Complete {
            value: Number(0.0),
            ..
        }
    ));
    let address = "A5".parse::<crate::model::CellAddress>().unwrap();
    book.sheet_mut(0).unwrap().set_value(address, 4.0).unwrap();
    assert!(matches!(
        evaluate(&book, query),
        EvaluationOutcome::Complete {
            value: Number(4.0),
            ..
        }
    ));
    book.sheet_mut(0).unwrap().remove(address).unwrap();
    assert!(matches!(
        evaluate(&book, query),
        EvaluationOutcome::Complete {
            value: Number(0.0),
            ..
        }
    ));
}

#[test]
fn whole_column_blank_coercion_and_error_order_are_preserved() {
    let mut book = book();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_value("A1", true).unwrap();
    sheet.set_value("A2", "2").unwrap();
    sheet.set_value("A3", 3.0).unwrap();
    sheet.set("A4", crate::model::Cell::default()).unwrap();
    sheet.set_formula("A5", "\"\"").unwrap();
    for (formula, expected) in [
        ("SUM(A:A)", Number(3.0)),
        ("COUNTBLANK(A:A)", Number(f64::from(MAX_ROWS - 3))),
        ("SUM((A3:A5,A3:A5))", Number(6.0)),
    ] {
        assert!(
            matches!(evaluate(&book, formula),
            EvaluationOutcome::Complete { value, .. } if value == expected),
            "{formula}"
        );
    }
    book.sheet_mut(0)
        .unwrap()
        .set_value("A1", Error(NA))
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_value("A2", Error(Div0))
        .unwrap();
    assert!(matches!(
        evaluate(&book, "SUM(A:A)"),
        EvaluationOutcome::Complete {
            value: Error(NA),
            ..
        }
    ));
}

#[test]
fn recycled_storage_rescans_overlapping_ranges_and_changed_inputs() {
    let mut workbook = book();
    let sheet = workbook.sheet_mut(0).unwrap();
    for row in 0..32u32 {
        sheet.set_value((row, 0), f64::from(row + 1)).unwrap();
        sheet.set_value((row, 1), 50.0).unwrap();
    }
    let limits = EvaluationBudgets {
        run_work: u64::MAX,
        cell_work: u64::MAX,
        materialized_values: usize::MAX,
        ..EvaluationBudgets::default()
    };
    let mut run = Session::new(
        &workbook,
        EvaluationInputs::default(),
        BudgetMeter::configured(limits, false).unwrap(),
    );
    for (last, first_value) in [(8, 1.0), (20, 1.0), (19, 1.0), (19, 1.0)] {
        let values = ItemFolder::new(&mut run)
            .items(Value::Refs(vec![area(0, last, 0, 1)].into()))
            .unwrap();
        let expected = (0..=last)
            .flat_map(|row| {
                [
                    (Number(f64::from(row) + first_value), true),
                    (Number(50.0), true),
                ]
            })
            .collect::<Vec<_>>();
        assert_eq!(values, expected);
        assert!(run.reuse.sparse_members.is_empty());
        assert!(run.reuse.sparse_members.capacity() >= expected.len());
    }
    for (first, last) in [(2, 23), (4, 20), (7, 27), (30, 31), (0, 19)] {
        let area = area(first, last, 0, 1);
        let values = ItemFolder::new(&mut run)
            .items(Value::Refs(vec![area].into()))
            .unwrap();
        let expected: Vec<_> = (first..=last)
            .flat_map(|row| [(Number(f64::from(row + 1)), true), (Number(50.0), true)])
            .collect();
        assert_eq!(values, expected);
    }
    let inputs = crate::eval::inputs::RunInputs::new(
        &workbook,
        [(0, CellAddress { row: 0, col: 0 }, Number(999.0))],
        1,
        std::sync::Arc::new(()),
    )
    .unwrap();
    run.sources.inputs = Some(&inputs);
    let values = ItemFolder::new(&mut run)
        .items(Value::Refs(vec![area(0, 19, 0, 1)].into()))
        .unwrap();
    assert_eq!(values[0], (Number(999.0), true));
    assert_eq!(values[38], (Number(20.0), true));
}

#[test]
fn recycled_storage_preserves_spill_discovery_and_limits() {
    let mut workbook = book();
    let sheet = workbook.sheet_mut(0).unwrap();
    for row in 0..17u32 {
        sheet.set_value((row, 0), f64::from(row + 1)).unwrap();
        sheet.set_value((row, 1), 50.0).unwrap();
    }
    sheet.set_formula("B25", "SEQUENCE(4)").unwrap();
    let open_limits = EvaluationBudgets {
        run_work: u64::MAX,
        cell_work: u64::MAX,
        materialized_values: usize::MAX,
        ..EvaluationBudgets::default()
    };
    let begin = || {
        Session::new(
            &workbook,
            EvaluationInputs::default(),
            BudgetMeter::configured(open_limits, false).unwrap(),
        )
    };
    let narrow = area(0, 16, 0, 1);
    let mut run = begin();
    let original = ItemFolder::new(&mut run)
        .items(Value::Refs(vec![narrow].into()))
        .unwrap();
    assert_eq!(original.len(), 34);
    assert!(run.reuse.sparse_members.is_empty());
    assert!(run.reuse.sparse_members.capacity() >= 34);

    let mut expected = original.clone();
    expected.extend([
        (Number(1.0), true),
        (Number(2.0), true),
        (Number(3.0), true),
    ]);
    let widened = ItemFolder::new(&mut run)
        .items(Value::Refs(vec![area(0, 26, 0, 1)].into()))
        .unwrap();
    assert_eq!(widened, expected);
    assert!(run.reuse.sparse_members.is_empty());

    let mut limited = begin();
    let _ = ItemFolder::new(&mut limited)
        .items(Value::Refs(vec![narrow].into()))
        .unwrap();
    assert!(limited.reuse.sparse_members.capacity() >= 34);
    limited.state.runtime.budget = BudgetMeter::configured(
        EvaluationBudgets {
            materialized_values: 2,
            ..open_limits
        },
        false,
    )
    .unwrap();
    assert!(matches!(
        SparseReader::new(&mut limited).sparse_members(area(2, 16, 0, 1)),
        Err(EvaluationError::Budget(BudgetError {
            kind: BudgetKind::Materialization,
            limit: 2,
            requested: 3,
            ..
        }))
    ));

    let collect_profiled = |session: &mut Session<'_>| {
        session.state.runtime.budget = BudgetMeter::configured(open_limits, true).unwrap();
        let values = ItemFolder::new(session)
            .items(Value::Refs(vec![area(2, 16, 0, 1)].into()))
            .unwrap();
        let usage = &session.state.runtime.budget.usage;
        (values, usage.sparse_entries, usage.materialized_values)
    };
    let mut with_cache = begin();
    ItemFolder::new(&mut with_cache)
        .items(Value::Refs(vec![narrow].into()))
        .unwrap();
    assert!(with_cache.reuse.sparse_members.capacity() >= 34);
    let mut without_cache = begin();
    ItemFolder::new(&mut without_cache)
        .items(Value::Refs(vec![narrow].into()))
        .unwrap();
    // Both runs have identical prior spill discovery and memo state. Remove
    // only the spare allocation, then compare the newly metered reads.
    without_cache.reuse.sparse_members = Vec::new();
    let uncached = collect_profiled(&mut without_cache);
    let cached = collect_profiled(&mut with_cache);
    assert_eq!(cached, uncached);
    assert_eq!(cached.0.len(), 30);
    assert!(cached.1 >= 30);
    assert!(cached.2 >= 30);
}

#[test]
fn recycled_member_storage_carries_no_range_state() {
    let mut book = book();
    let sheet = book.sheet_mut(0).unwrap();
    sheet
        .set_value(CellAddress { row: 1, col: 0 }, 8.0)
        .unwrap();
    sheet
        .set_value(CellAddress { row: 2, col: 1 }, 13.0)
        .unwrap();
    let mut run = session(&book, EvaluationBudgets::default());
    for (range, expected) in [(area(0, 3, 0, 0), 8.0), (area(2, 3, 1, 1), 13.0)] {
        let seen = ItemFolder::new(&mut run)
            .items(Value::Refs(vec![range].into()))
            .unwrap();
        assert_eq!(seen, vec![(Number(expected), true)]);
        assert!(run.reuse.sparse_members.is_empty());
        assert!(run.reuse.sparse_members.capacity() >= 1);
    }
}

#[test]
fn internal_point_recording_preserves_observed_order_and_retained_evidence() {
    use super::super::read_recorder::ReadRecorder;
    use crate::eval::coordinates::EvaluationTarget;
    use crate::eval::ports::{DependencyRead, DependencyReadSink, ReadEvent};
    use crate::eval::reuse::Reads;

    /// Capture point reads forwarded through the public dependency port.
    #[derive(Default)]
    struct Events(Vec<ReadEvent>);

    impl DependencyReadSink for Events {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event);
        }
    }

    let book = book();
    let mut events = Events::default();
    let mut internal = session(&book, EvaluationBudgets::default());
    let mut observed = session(&book, EvaluationBudgets::default());
    internal.reuse.overlay_reads = Some(Reads::default());
    observed.reuse.overlay_reads = Some(Reads::default());
    observed.ports.dependency_sink = Some(&mut events);

    let target = |row, col| EvaluationTarget {
        sheet: 0,
        address: CellAddress { row, col },
    };
    let consumer = Some(target(4, 4));
    let another = Some(target(5, 4));
    let reads = [
        (None, target(0, 0)),
        (consumer, target(0, 0)),
        (consumer, target(0, 1)),
        (another, target(0, 3)),
        (consumer, target(0, 0)),
        (consumer, target(0, 2)),
    ];
    for (consumer, point) in reads {
        for session in [&mut internal, &mut observed] {
            session.state.runtime.stack.consumer = consumer;
            ReadRecorder::new(session).read_cell(point);
        }
    }
    let traces = |mut reads: Reads| {
        reads.flush();
        reads.traces
    };
    assert_eq!(
        traces(internal.reuse.overlay_reads.take().unwrap()),
        traces(observed.reuse.overlay_reads.take().unwrap())
    );
    drop(observed);
    assert_eq!(
        events.0,
        reads.map(|(consumer, point)| ReadEvent {
            consumer,
            read: DependencyRead::Cell(point),
        })
    );
}
