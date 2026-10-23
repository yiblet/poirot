//! Sparse coordinate collection completes before evaluating referenced cell bodies.
//! SUM's ignored reference values and error propagation follow Microsoft's contract:
//! https://support.microsoft.com/en-us/excel/functions/sum-function
//! Ordering, budget attribution and work counters are evaluator contracts.
use poirot_eval::*;
use poirot_model::{CellError, CellValue, Workbook, query::RangeMembership};

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
    let policy = EvaluationPolicy::default();
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
        let mut cells = poirot_model::query::WorkbookView::new(&book)
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
        assert_eq!(
            usage.sparse_entries,
            column_work.unwrap_or(visits),
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
    // Five earlier admissions are two endpoint descriptors, the combined area,
    // and two dynamic-spill membership guards. No cell body has run yet.
    assert_eq!(usage.materialized_values, 5 + 2);
    // One column-index seek + one page seek + three stored payload visits.
    assert_eq!(usage.sparse_entries, 5);
    assert_eq!(usage.cell_reads, 1);
    assert_eq!(usage.completed_cells, 0);
    assert_eq!(observed, vec![at("Z1")]);
}

#[test]
fn finite_work_admits_prefetched_page_visits_before_referenced_cell_bodies() {
    let book = formula_sources();
    // Before collection: one root read + ten formula bytes + four expressions
    // + three discovery candidates + five materializations = 23 work units.
    // First step loads one page and seeks the column index, then prefetches
    // two stored cells (4 units). The second step prefetches the third (1 unit).
    // Each returned coordinate then needs one materialization; no extra tree
    // seek occurs on terminal exhaustion of this already loaded page.
    for (limit, sparse, admitted, requested) in [
        (25, 2, 0, 2),
        (27, 4, 0, 1),
        (28, 4, 1, 1),
        (29, 5, 1, 1),
        (30, 5, 2, 1),
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
        assert_eq!(usage.work_units, 23 + sparse + admitted);
        assert_eq!(usage.sparse_entries, sparse);
        assert_eq!(usage.materialized_values, 5 + admitted);
        assert_eq!(usage.cell_reads, 1);
        assert_eq!(usage.completed_cells, 0);
        assert_eq!(observed, vec![at("Z1")]);
    }
    // The root read precedes the formula's frame, so its direct-work allowance
    // of 29 reaches the same third-coordinate rejection as a run allowance of 30.
    let (report, observed) = calculate(
        &book,
        EvaluationBudgets {
            cell_work: 29,
            ..Default::default()
        },
    );
    let (failure, usage) = aborted(report);
    assert_eq!(
        failure,
        BudgetFailure {
            kind: BudgetKind::CellWork,
            limit: 29,
            consumed: 29,
            requested: 1,
            target: Some(at("Z1"))
        }
    );
    assert_eq!(usage.work_units, 30);
    assert_eq!(usage.sparse_entries, 5);
    assert_eq!(usage.materialized_values, 7);
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
    assert_eq!(report.outcome.usage().sparse_entries, 5);
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
    assert_eq!(usage.sparse_entries, 5);
    assert_eq!(usage.maximum_depth, 4);
    assert_eq!(observed, vec![at("A1"), at("Z1")]);
}
