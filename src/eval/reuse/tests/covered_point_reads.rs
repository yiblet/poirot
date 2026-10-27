use super::super::dependencies::Dependencies;
use super::super::key::Key;
use super::super::reads::Reads;
use crate::eval::ports::{DependencyRead, ReadEvent};
use crate::model::query::RangeMembership;
use crate::model::{CellAddress, CellValue};

use crate::model::Workbook;

fn key(sheet: usize, row: u32, col: u32) -> Key {
    (sheet, CellAddress { row, col })
}

fn membership(sheet: usize, top: u32, left: u32, bottom: u32, right: u32) -> RangeMembership {
    RangeMembership::new(sheet, key(sheet, top, left).1, key(sheet, bottom, right).1).unwrap()
}

#[test]
fn suppressing_covered_reads_preserves_the_independent_dependency_union() {
    // The oracle records explicit points plus inclusive rectangle inequalities,
    // independently of the production compression and RangeMembership::contains.
    let mut deps = Dependencies::default();
    let mut points = Vec::new();
    let mut rectangles = Vec::new();
    for (sheet, top, left, bottom, right) in [
        (0, 1, 1, 4, 3),
        (0, 3, 2, 6, 5),
        (0, 1, 6, 4, 7),
        (1, 1, 1, 4, 3),
    ] {
        // Points before membership must remain safe; points after membership
        // include overlaps, adjacent coordinates, other sheets and outside owners.
        for at in [key(sheet, top, left), key(sheet, top, right + 1)] {
            deps.cell(at);
            points.push(at);
        }
        deps.ranges
            .push(membership(sheet, top, left, bottom, right));
        rectangles.push((sheet, top, left, bottom, right));
        let old_runs = deps.point_runs.len();
        let old_points = deps.cells.len();
        for row in top..=bottom {
            for col in left..=right {
                deps.cell(key(sheet, row, col));
            }
        }
        assert_eq!(deps.point_runs.len(), old_runs);
        assert_eq!(deps.cells.len(), old_points);
        for at in [
            key(sheet, 0, 0),
            key(sheet, bottom + 1, right),
            key(2, top, left),
        ] {
            deps.cell(at);
            points.push(at);
        }
    }
    for sheet in 0..4 {
        for row in 0..9 {
            for col in 0..10 {
                let at = key(sheet, row, col);
                let expected = points.contains(&at)
                    || rectangles.iter().any(|&(s, t, l, b, r)| {
                        s == sheet && t <= row && row <= b && l <= col && col <= r
                    });
                let actual = deps.cells.contains(&at)
                    || deps
                        .ranges
                        .iter()
                        .chain(&deps.point_runs)
                        .any(|r| r.contains(sheet, at.1));
                assert_eq!(actual, expected, "{at:?}");
            }
        }
    }
}

#[test]
fn older_membership_and_gaps_conservatively_keep_point_edges() {
    let mut deps = Dependencies::default();
    deps.ranges.push(membership(0, 1, 1, 3, 1));
    deps.ranges.push(membership(0, 1, 3, 3, 3));
    let old_range_point = key(0, 2, 1);
    let gap = key(0, 2, 2);
    deps.cell(old_range_point);
    assert_eq!(deps.cells, vec![old_range_point]);
    deps.cell(gap);
    // These points may compress together, but the gap must never be dropped.
    assert!(deps.cells.contains(&gap) || deps.point_runs.iter().any(|r| r.contains(gap.0, gap.1)));
    assert!(
        deps.cells.contains(&old_range_point)
            || deps
                .point_runs
                .iter()
                .any(|r| r.contains(old_range_point.0, old_range_point.1))
    );
}

#[test]
fn internal_compaction_keeps_external_events_and_work_identical() {
    use crate::eval::compile::compiler::FormulaCompiler;
    use crate::eval::ports::DependencyReadSink;
    use crate::eval::run::{EvaluationRun, WorkbookView};
    /// Retain callbacks for an exact ordered comparison.
    #[derive(Default)]
    struct Events(Vec<ReadEvent>);
    impl DependencyReadSink for Events {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event);
        }
    }
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value(key(0, 0, 0).1, 3.).unwrap();
    sheet.set_formula(key(0, 1, 0).1, "C1*2").unwrap();
    sheet.set_value(key(0, 0, 2).1, 4.).unwrap();
    sheet.set_formula(key(0, 0, 5).1, "SUM(A1:A3)+C1").unwrap();
    let policy = crate::eval::compile::compiler::PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
    let mut observed = Vec::new();
    for with_internal in [false, true] {
        let mut events = Events::default();
        let mut run = EvaluationRun::new(
            WorkbookView::new(&book, &plan, &policy).unwrap(),
            crate::eval::config::EvaluationInputs::default(),
            crate::eval::limits::EvaluationBudgets::default(),
        )
        .unwrap()
        .with_dependency_sink(&mut events);
        if with_internal {
            run.session.reuse.overlay_reads = Some(Reads::default());
        }
        let value = run
            .evaluate_cell(crate::eval::coordinates::EvaluationTarget {
                sheet: 0,
                address: key(0, 0, 5).1,
            })
            .unwrap();
        assert_eq!(value, CellValue::Number(15.));
        let usage = run.finish(Ok(())).execution;
        observed.push((events.0, usage));
    }
    assert_eq!(observed[0], observed[1]);
    assert!(
        observed[0]
            .0
            .iter()
            .any(|e| matches!(e.read, DependencyRead::SparseMembership { .. }))
    );
    assert!(observed[0].0.iter().any(|e| e.consumer
        == Some(crate::eval::coordinates::EvaluationTarget {
            sheet: 0,
            address: key(0, 0, 5).1
        })
        && e.read
            == DependencyRead::Cell(crate::eval::coordinates::EvaluationTarget {
                sheet: 0,
                address: key(0, 1, 0).1
            })));
}

#[test]
fn nested_consumers_keep_their_own_guards_and_outside_reads() {
    use crate::eval::ports::DependencyReadSink;
    let parent = key(0, 10, 10);
    let child = key(0, 2, 1);
    let owner = key(0, 0, 0);
    let mut reads = Reads::default();
    let emit = |reads: &mut Reads, consumer: Key, read| {
        reads.read(ReadEvent {
            consumer: Some(crate::eval::coordinates::EvaluationTarget {
                sheet: consumer.0,
                address: consumer.1,
            }),
            read,
        });
    };
    emit(
        &mut reads,
        parent,
        DependencyRead::SparseMembership {
            sheet: 0,
            first: key(0, 1, 1).1,
            last: key(0, 4, 1).1,
        },
    );
    emit(
        &mut reads,
        child,
        DependencyRead::Cell(crate::eval::coordinates::EvaluationTarget {
            sheet: 0,
            address: owner.1,
        }),
    );
    emit(
        &mut reads,
        parent,
        DependencyRead::Cell(crate::eval::coordinates::EvaluationTarget {
            sheet: 0,
            address: child.1,
        }),
    );
    emit(
        &mut reads,
        parent,
        DependencyRead::Cell(crate::eval::coordinates::EvaluationTarget {
            sheet: 0,
            address: owner.1,
        }),
    );
    // Adjacent membership merges do not invalidate the already covered guard.
    emit(
        &mut reads,
        parent,
        DependencyRead::SparseMembership {
            sheet: 0,
            first: key(0, 5, 1).1,
            last: key(0, 6, 1).1,
        },
    );
    emit(
        &mut reads,
        parent,
        DependencyRead::Cell(crate::eval::coordinates::EvaluationTarget {
            sheet: 0,
            address: key(0, 6, 1).1,
        }),
    );
    reads.flush();
    let parent_trace = reads.traces.get(&parent).unwrap();
    assert_eq!(parent_trace.cells, vec![owner]);
    assert!(parent_trace.point_runs.is_empty());
    assert_eq!(parent_trace.ranges.len(), 1);
    assert!(parent_trace.ranges[0].contains(0, key(0, 6, 1).1));
    assert_eq!(reads.traces.get(&child).unwrap().cells, vec![owner]);
}
