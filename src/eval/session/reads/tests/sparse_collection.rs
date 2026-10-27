//! Sparse coordinate collection completes before evaluating referenced cell bodies.
//! SUM's ignored reference values and error propagation follow Microsoft's contract:
//! https://support.microsoft.com/en-us/excel/functions/sum-function
//! Ordering, budget attribution and work counters are evaluator contracts.
use crate::eval::compile::compiler::{FormulaCompiler, PreparationPolicy};
use crate::eval::config::{ArrayResultPolicy, RunConfiguration};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{
    BudgetFailure, BudgetKind, EvaluationBudgets, EvaluationFailure, EvaluationOutcome,
    EvaluationUsage,
};
use crate::eval::ports::{CellCompletion, CellObserver};
use crate::eval::run::{EvaluationRun, RunReport, WorkbookView};
use crate::model::{CellError, CellValue, Workbook, query::RangeMembership};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

/// Capture the ordered cells whose results or operational failures were observed.
#[derive(Default)]
struct Completed(Vec<EvaluationTarget>);

impl CellObserver for Completed {
    fn completed(&mut self, event: CellCompletion<'_>) -> Result<(), EvaluationFailure> {
        self.0.push(event.target);
        Ok(())
    }
}

fn calculate(
    book: &Workbook,
    budgets: EvaluationBudgets,
) -> (RunReport<CellValue>, Vec<EvaluationTarget>) {
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(book, &policy);
    let configuration =
        RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::DynamicSpill);
    let mut observed = Completed::default();
    let mut run = EvaluationRun::new_with_configuration(
        WorkbookView::new(book, &plan, &policy).unwrap(),
        &configuration,
        budgets,
    )
    .unwrap()
    .with_cell_observer(&mut observed);
    let result = run.evaluate_cell(at("Z1"));
    (run.finish(result), observed.0)
}

#[test]
fn coordinate_scans_include_terminal_seeks_without_duplicate_traversal() {
    for (range, entries, expected, column_work) in [
        (
            "A1:B2",
            vec![("A1", 1.), ("B1", 2.), ("A2", 3.), ("B2", 4.)],
            10.,
            None,
        ),
        ("A1:B8", vec![("A1", 1.), ("B5", 4.), ("C7", 99.)], 5., None),
        ("A1:A8", vec![("B2", 99.), ("B7", 99.)], 0., Some(2)),
        ("A1:B8", vec![], 0., None),
    ] {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        for (address, value) in entries {
            sheet.set_value(at(address).address, value).unwrap();
        }
        sheet
            .set_formula(at("Z1").address, format!("SUM({range})"))
            .unwrap();
        let (first, last) = range.split_once(':').unwrap();
        let membership = RangeMembership::new(0, at(first).address, at(last).address).unwrap();
        let mut cells = crate::model::query::WorkbookView::new(&book)
            .stored(membership)
            .unwrap();
        while cells.next().is_some() {}
        let counters = cells.counters();
        let visits = counters.row_seeks + counters.column_seeks + counters.cells_visited;
        let (report, _) = calculate(&book, Default::default());
        let EvaluationOutcome::Complete { value, usage } = report.outcome else {
            panic!("unexpected abort");
        };
        assert_eq!(value, CellValue::Number(expected), "{range}");
        // Rectangles use the same occupied-row algorithm as model queries.
        // The empty A column instead opens its column index and loads one 32x4
        // page occupied by B; bounded local slots contain no selected payloads.
        // SUM is proven scalar and blocks other producers at Z1, so discovery
        // inspects only the referenced range's prefix, excluding C7 beside A:B.
        let discovered = book
            .sheet(0)
            .unwrap()
            .cells()
            .iter()
            .filter(|(address, _)| {
                address.row <= membership.last.row && address.col <= membership.last.col
            })
            .count() as u64;
        assert_eq!(
            usage.sparse_entries,
            column_work.unwrap_or(visits) + discovered,
            "{range}"
        );
    }
}

fn formula_sources() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula(at("A1").address, "1/0").unwrap();
    sheet.set_formula(at("A2").address, "2+3").unwrap();
    sheet
        .set_value(at("A3").address, CellValue::Error(CellError::Ref))
        .unwrap();
    sheet.set_formula(at("Z1").address, "SUM(A1:A5)").unwrap();
    book
}

fn aborted(report: RunReport<CellValue>) -> (BudgetFailure, EvaluationUsage) {
    match report.outcome {
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(failure),
            usage,
        } => (failure, usage),
        other => panic!("expected a metered failure, got {other:?}"),
    }
}

#[test]
fn collection_limit_rejects_before_admitting_coordinate_or_executing_cell_bodies() {
    let (report, observed) = calculate(
        &formula_sources(),
        EvaluationBudgets {
            materialized_values: 2,
            ..Default::default()
        },
    );
    assert_eq!(report.execution.formula_executions, 1);
    let (failure, usage) = aborted(report);
    assert_eq!(
        failure,
        BudgetFailure {
            kind: BudgetKind::Materialization,
            limit: 2,
            consumed: 0,
            requested: 3,
            target: Some(at("Z1"))
        }
    );
    // Two coordinates were admitted; the third was visited but never materialized.
    // Three earlier admissions are two endpoint descriptors and the combined
    // area. Stable areas need no phase-only self-footprint guards.
    assert_eq!(usage.materialized_values, 3 + 2);
    // Three source inspections, one column seek, one page seek and three payload visits.
    assert_eq!(usage.sparse_entries, 8);
    assert_eq!(usage.cell_reads, 1);
    assert_eq!(usage.completed_cells, 0);
    assert_eq!(observed, vec![at("Z1")]);
}

#[test]
fn finite_work_admits_prefetched_page_visits_before_referenced_cell_bodies() {
    let book = formula_sources();
    // Before collection: one root read + ten formula bytes + four expressions
    // + three range-source inspections + three materializations = 21 work units.
    // The scalar root needs no origin-prefix inspection of its own; all three
    // source inspections occur inside its formula-work frame (20 direct units).
    // First step loads one page and seeks the column index, then prefetches
    // two stored cells (4 units). The second step prefetches the third (1 unit).
    // Each returned coordinate then needs one materialization; no extra tree
    // seek occurs on terminal exhaustion of this already loaded page.
    for (limit, sparse, admitted, requested) in [
        (23, 2, 0, 2),
        (25, 4, 0, 1),
        (26, 4, 1, 1),
        (27, 5, 1, 1),
        (28, 5, 2, 1),
    ] {
        let (report, observed) = calculate(
            &book,
            EvaluationBudgets {
                run_work: limit,
                ..Default::default()
            },
        );
        assert_eq!(report.execution.formula_executions, 1);
        let (failure, usage) = aborted(report);
        assert_eq!(
            failure,
            BudgetFailure {
                kind: BudgetKind::RunWork,
                limit,
                consumed: limit,
                requested,
                target: Some(at("Z1"))
            }
        );
        assert_eq!(usage.work_units, 21 + sparse + admitted);
        assert_eq!(usage.sparse_entries, 3 + sparse);
        assert_eq!(usage.materialized_values, 3 + admitted);
        assert_eq!(usage.cell_reads, 1);
        assert_eq!(usage.completed_cells, 0);
        assert_eq!(observed, vec![at("Z1")]);
    }
    // The former smaller allowance still refuses before referenced bodies run.
    let (report, observed) = calculate(
        &book,
        EvaluationBudgets {
            cell_work: 24,
            ..Default::default()
        },
    );
    assert_eq!(report.execution.formula_executions, 1);
    let (failure, _) = aborted(report);
    assert_eq!(failure.kind, BudgetKind::CellWork);
    assert_eq!((failure.limit, failure.consumed), (24, 24));
    assert_eq!(observed, vec![at("Z1")]);
    // A1, A2 and A3 are discovered inside the root formula's frame. A 27-unit
    // formula allowance reaches the final-coordinate rejection, matching the
    // 28-unit run allowance after including the root's initial cell read.
    let (report, observed) = calculate(
        &book,
        EvaluationBudgets {
            cell_work: 27,
            ..Default::default()
        },
    );
    let (failure, usage) = aborted(report);
    assert_eq!(
        failure,
        BudgetFailure {
            kind: BudgetKind::CellWork,
            limit: 27,
            consumed: 27,
            requested: 1,
            target: Some(at("Z1"))
        }
    );
    assert_eq!(usage.work_units, 28);
    assert_eq!(usage.sparse_entries, 8);
    assert_eq!(usage.materialized_values, 5);
    assert_eq!(observed, vec![at("Z1")]);
}

#[test]
fn completed_collection_preserves_error_order_and_nested_failure_attribution() {
    let book = formula_sources();
    let (report, observed) = calculate(&book, Default::default());
    assert!(matches!(
        report.outcome,
        EvaluationOutcome::Complete {
            value: CellValue::Error(CellError::Div0),
            ..
        }
    ));
    assert_eq!(report.outcome.usage().sparse_entries, 8);
    assert_eq!(observed, vec![at("A1"), at("A2"), at("A3"), at("Z1")]);
    let (report, observed) = calculate(
        &book,
        EvaluationBudgets {
            depth: 4,
            ..Default::default()
        },
    );
    let (failure, usage) = aborted(report);
    assert_eq!(
        failure,
        BudgetFailure {
            kind: BudgetKind::Depth,
            limit: 4,
            consumed: 4,
            requested: 1,
            target: Some(at("A1"))
        }
    );
    assert_eq!(usage.sparse_entries, 8);
    assert_eq!(usage.maximum_depth, 4);
    assert_eq!(observed, vec![at("A1"), at("Z1")]);
}
