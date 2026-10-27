//! Prefix reuse must preserve lazy providers, bounds, type identity and run isolation.
use crate::eval::compile;
use crate::eval::config::{ArrayResultPolicy, EvaluationInputs};
use crate::eval::ports::{DependencyRead, DependencyReadSink, ReadEvent};
use crate::eval::session::{ResultProjection, Session};
use crate::eval::vm::ExpressionRunner;
use crate::model::CellError::NA;
use crate::model::CellValue::{Boolean, Error, Number};
use crate::model::{CellValue, Workbook};

fn book() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    workbook
}

fn evaluate(session: &mut Session<'_>, formula: &str) -> CellValue {
    let parsed = poirot_parsing::formula::parse_formula(formula).unwrap();
    let bound = compile::bound::FormulaPlan::bind(&parsed).unwrap();
    let at = "Z20".parse().unwrap();
    let value = ExpressionRunner::new(session)
        .expr(0, at, &bound.expression())
        .unwrap();
    ResultProjection::new(session)
        .select_legacy_scalar(at, value)
        .unwrap()
}

fn session(workbook: &Workbook) -> Session<'_> {
    let mut session = Session::with_context(workbook, EvaluationInputs::default());
    session.policies.array_result_policy = ArrayResultPolicy::ScalarIntersection;
    session.state.runtime.lookup_prefixes.enable();
    session
}

#[test]
fn repeated_exact_queries_share_keys_but_keep_first_duplicate_and_range_bounds() {
    let mut w = book();
    for (cell, value) in [
        ("A1", 1.),
        ("A2", 2.),
        ("A3", 2.),
        ("B2", 20.),
        ("C2", 200.),
        ("B3", 30.),
    ] {
        w.sheet_mut(0).unwrap().set_value(cell, value).unwrap();
    }
    w.sheet_mut(0)
        .unwrap()
        .set_formula("A4", "UNSUPPORTED()")
        .unwrap();
    let mut run = session(&w);
    assert_eq!(evaluate(&mut run, "VLOOKUP(2,A1:B4,2,FALSE)"), Number(20.));
    let reads = run.state.runtime.budget.usage.cell_reads;
    assert_eq!(evaluate(&mut run, "VLOOKUP(2,A1:C4,3,FALSE)"), Number(200.));
    assert_eq!(run.state.runtime.budget.usage.cell_reads - reads, 1);
    assert_eq!(run.state.runtime.run_usage.formula_executions, 0);
    assert_eq!(evaluate(&mut run, "MATCH(2,A3:A4,0)"), Number(1.));
    assert_eq!(run.state.runtime.run_usage.formula_executions, 0);
}

#[test]
fn zero_boolean_and_absent_candidates_remain_distinct_and_misses_can_be_reused() {
    let mut w = book();
    for (cell, value) in [
        ("A2", Number(-0.)),
        ("A3", Boolean(false)),
        ("A4", Number(4.)),
    ] {
        w.sheet_mut(0).unwrap().set_value(cell, value).unwrap();
    }
    let mut run = session(&w);
    assert_eq!(evaluate(&mut run, "MATCH(0,A1:A5,0)"), Number(2.));
    assert_eq!(evaluate(&mut run, "MATCH(FALSE,A1:A5,0)"), Number(3.));
    let reads = run.state.runtime.budget.usage.cell_reads;
    assert_eq!(evaluate(&mut run, "MATCH(-0,A1:A5,0)"), Number(2.));
    assert_eq!(evaluate(&mut run, "MATCH(FALSE,A1:A5,0)"), Number(3.));
    assert_eq!(run.state.runtime.budget.usage.cell_reads, reads);
    assert_eq!(evaluate(&mut run, "MATCH(9,A1:A5,0)"), Error(NA));
    let reads = run.state.runtime.budget.usage.cell_reads;
    assert_eq!(evaluate(&mut run, "MATCH(9,A1:A5,0)"), Error(NA));
    assert_eq!(run.state.runtime.budget.usage.cell_reads, reads);
    // A different source/run has no retained miss or first-match position.
    drop(run);
    w.sheet_mut(0).unwrap().set_value("A1", 9.).unwrap();
    assert_eq!(evaluate(&mut session(&w), "MATCH(9,A1:A5,0)"), Number(1.));
}

#[test]
fn horizontal_prefixes_and_overlaid_key_cells_are_case_local() {
    let mut w = book();
    for (cell, value) in [
        ("A1", 1.),
        ("B1", 2.),
        ("C1", 3.),
        ("B2", 20.),
        ("C2", 30.),
        ("C3", 300.),
    ] {
        w.sheet_mut(0).unwrap().set_value(cell, value).unwrap();
    }
    let input = crate::eval::inputs::RunInputs::new(
        &w,
        [(0, "C1".parse().unwrap(), Number(2.))],
        1,
        std::sync::Arc::new(()),
    )
    .unwrap();
    let mut run = session(&w);
    run.sources.inputs = Some(&input);
    assert_eq!(evaluate(&mut run, "HLOOKUP(2,A1:C2,2,FALSE)"), Number(20.));
    assert_eq!(evaluate(&mut run, "MATCH(2,C1:C1,0)"), Number(1.));
    assert_eq!(evaluate(&mut session(&w), "MATCH(2,C1:C1,0)"), Error(NA));
}

#[test]
fn errored_prefixes_and_reentrant_lookup_providers_do_not_poison_later_selection() {
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_formula("A1", "IFERROR(MATCH(7,A1:A2,0),0)")
        .unwrap();
    w.sheet_mut(0).unwrap().set_value("A2", 7.).unwrap();
    let mut indexed = session(&w);
    let mut streamed = Session::with_context(&w, EvaluationInputs::default());
    for formula in ["MATCH(7,A1:A2,0)", "MATCH(2,A1:A2,0)", "MATCH(7,A1:A2,0)"] {
        assert_eq!(
            evaluate(&mut indexed, formula),
            evaluate(&mut streamed, formula)
        );
    }
}

#[test]
fn observed_queries_keep_each_consumers_original_reads_and_small_caps_stream() {
    #[derive(Default)]
    /// Records each consumer's original reads when prefix acceleration is disabled.
    struct Reads(Vec<ReadEvent>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event);
        }
    }
    let mut w = book();
    for (cell, value) in [("A1", 1.), ("A2", 2.), ("B2", 20.)] {
        w.sheet_mut(0).unwrap().set_value(cell, value).unwrap();
    }
    let mut reads = Reads::default();
    let mut run = session(&w);
    run.ports.dependency_sink = Some(&mut reads);
    assert_eq!(evaluate(&mut run, "VLOOKUP(2,A1:B2,2,FALSE)"), Number(20.));
    let first = run.state.runtime.budget.usage.cell_reads;
    assert_eq!(evaluate(&mut run, "VLOOKUP(2,A1:B2,2,FALSE)"), Number(20.));
    assert_eq!(run.state.runtime.budget.usage.cell_reads - first, 3);
    drop(run);
    assert_eq!(
        reads
            .0
            .iter()
            .filter(|e| matches!(e.read, DependencyRead::Cell(_)))
            .count(),
        6
    );
    let mut run = session(&w);
    run.state.runtime.budget.limits.materialized_values = 2;
    for formula in ["MATCH(2,A1:A2,0)", "MATCH(1,A1:A2,0)"] {
        assert_eq!(
            evaluate(&mut run, formula),
            Number(if formula.contains("MATCH(2") { 2. } else { 1. })
        );
    }
}
