use super::super::comparison::lookup_compare;
use super::super::search_mode::SearchMode;
use super::super::search_selection::SearchSelection;
use super::super::selection::select_index;
use crate::eval::compile;
use crate::eval::config::{ArrayResultPolicy, EvaluationInputs};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::entry::run_formula;
use crate::eval::limits::{
    BudgetKind, BudgetMeter, EvaluationBudgets, EvaluationError, EvaluationOutcome,
};
use crate::eval::ports::{DependencyRead, DependencyReadSink, ReadEvent};
use crate::eval::session::{ResultProjection, Session};
use crate::eval::values::{FunctionResult, VALUE_ERROR};
use crate::eval::vm::ExpressionRunner;
use crate::model::CellError::{NA, Ref};
use crate::model::CellValue::{Blank, Error, Number, Text};
use crate::model::{CellAddress, CellValue, Workbook};

#[test]
fn boolean_match_modes_follow_numeric_modes_and_source_read_order() {
    /// Collects actual read events for lookup ordering assertions.
    #[derive(Default)]
    struct Reads(Vec<ReadEvent>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event);
        }
    }
    for (mode, expected, candidates) in [
        ("TRUE", Number(3.), vec!["B2", "B3", "B4"]),
        ("1", Number(3.), vec!["B2", "B3", "B4"]),
        ("FALSE", Number(2.), vec!["B1", "B2"]),
        ("0", Number(2.), vec!["B1", "B2"]),
        ("1/0", Number(99.), vec![]),
    ] {
        let mut w = Workbook::new();
        w.add_sheet("Sheet1").unwrap();
        let sheet = w.sheet_mut(0).unwrap();
        sheet
            .set_formula_with_cached_value("C1", mode, Number(-999.))
            .unwrap();
        sheet
            .set_formula_with_cached_value("A1", "1+1", Number(-999.))
            .unwrap();
        for (cell, value) in [("B1", 1.), ("B2", 2.), ("B3", 2.), ("B4", 3.)] {
            sheet.set_value(cell, value).unwrap();
        }
        let syntax =
            poirot_parsing::formula::parse_formula("IFERROR(MATCH(A1,B1:B4,C1),99)").unwrap();
        let selected = compile::bound::FormulaPlan::bind(&syntax).unwrap();
        let mut reads = Reads::default();
        let mut session = Session::with_context(&w, EvaluationInputs::default());

        session.policies.array_result_policy = ArrayResultPolicy::ScalarIntersection;
        session.ports.dependency_sink = Some(&mut reads);
        let at = "Z20".parse().unwrap();
        let value = ExpressionRunner::new(&mut session)
            .expr(0, at, &selected.expression())
            .unwrap();
        assert_eq!(
            ResultProjection::new(&mut session)
                .select_legacy_scalar(at, value)
                .unwrap(),
            expected,
            "{mode}"
        );
        assert_eq!(
            session.state.runtime.run_usage.formula_executions,
            if mode == "1/0" { 1 } else { 2 }
        );
        assert_eq!(
            session
                .state
                .runtime
                .memo
                .cache
                .contains_key(&(0, "A1".parse().unwrap())),
            mode != "1/0"
        );
        drop(session);
        let cells: Vec<_> = reads
            .0
            .into_iter()
            .filter_map(|event| match event.read {
                DependencyRead::Cell(target) => Some(target),
                _ => None,
            })
            .collect();
        let mut expected_reads = vec!["C1"];
        if mode != "1/0" {
            expected_reads.push("A1");
        }
        expected_reads.extend(candidates);
        assert_eq!(
            cells,
            expected_reads
                .into_iter()
                .map(|cell| EvaluationTarget {
                    sheet: 0,
                    address: cell.parse().unwrap(),
                })
                .collect::<Vec<_>>(),
            "{mode}"
        );
    }
}

fn select_match(
    key: &CellValue,
    candidates: &[CellValue],
    mode: SearchMode,
) -> FunctionResult<Option<SearchSelection>> {
    {
        let mut budget = BudgetMeter::new(EvaluationBudgets::default()).unwrap();
        select_index(candidates.len(), mode, false, |i| {
            lookup_compare(key, &candidates[i], mode, true, &mut budget)
        })
        .map(|i| i.map(|index| SearchSelection { index }))
    }
}

#[test]
fn sheet_spans_use_source_specific_vector_geometry() {
    let mut w = Workbook::new();
    for name in ["First", "Last"] {
        w.add_sheet(name).unwrap();
    }
    w.sheet_mut(0).unwrap().set_value("A1", 7.).unwrap();
    w.sheet_mut(0).unwrap().set_value("B1", 42.).unwrap();
    w.sheet_mut(1).unwrap().set_value("A1", 9.).unwrap();
    for (formula, expected) in [
        ("MATCH(9,First:Last!A1,0)", Number(2.)),
        ("VLOOKUP(7,First:Last!A:B,2,FALSE)", Number(42.)),
        ("MATCH(7,First:Last!A1:A2,0)", Number(1.)),
        (
            "VLOOKUP(7,(First!A1:B1,Last!A1:B1),2,FALSE)",
            Error(VALUE_ERROR),
        ),
    ] {
        assert_eq!(
            match crate::eval::entry::run_formula(
                &w,
                0,
                "Z20".parse().unwrap(),
                formula,
                Default::default(),
                Default::default()
            ) {
                crate::eval::limits::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{formula}"
        );
    }
}

#[test]
fn whole_columns_read_only_selected_prefix_and_result() {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    let sheet = w.sheet_mut(0).unwrap();
    sheet.set_value("A1", 7.).unwrap();
    sheet
        .set_formula_with_cached_value("B1", "6*7", Number(-99.))
        .unwrap();
    for (formula, expected) in [
        ("MATCH(7,A:A,0)", Number(1.)),
        ("VLOOKUP(7,A:B,2,FALSE)", Number(42.)),
        ("HLOOKUP(7,1:3,3,FALSE)", Blank),
        ("VLOOKUP(8,A1:B1,1/0,FALSE)", Error(NA)),
        ("VLOOKUP(7,A1:B1,1/0,FALSE)", Error(Ref)),
    ] {
        assert!(
            matches!(run_formula(&w, 0, "Z20".parse().unwrap(), formula, EvaluationInputs::default(), EvaluationBudgets::default()), EvaluationOutcome::Complete { value, .. } if value == expected),
            "{formula}"
        );
    }
}

#[test]
fn exact_reads_do_not_consume_unused_formulas_or_errors() {
    #[derive(Default)]
    /// Records exact-match reads to detect accidental evaluation of unused candidates.
    struct Reads(Vec<ReadEvent>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event);
        }
    }
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    let sheet = w.sheet_mut(0).unwrap();
    sheet.set_value("A1", 7.).unwrap();
    sheet
        .set_formula_with_cached_value("B1", "6*7", Number(-99.))
        .unwrap();
    for (cell, formula) in [("A2", "RAND()"), ("B2", "UNSUPPORTED()"), ("B3", "1/0")] {
        sheet
            .set_formula_with_cached_value(cell, formula, Number(-99.))
            .unwrap();
    }
    let mut reads = Reads::default();
    let mut session = Session::with_context(&w, EvaluationInputs::default());
    session.ports.dependency_sink = Some(&mut reads);
    let syntax = poirot_parsing::formula::parse_formula("VLOOKUP(7,A:B,2,FALSE)").unwrap();
    let plan = compile::bound::FormulaPlan::bind(&syntax).unwrap();
    let at = "Z20".parse().unwrap();
    let value = ExpressionRunner::new(&mut session)
        .expr(0, at, &plan.expression())
        .unwrap();
    assert_eq!(
        ResultProjection::new(&mut session)
            .select_legacy_scalar(at, value)
            .unwrap(),
        Number(42.)
    );
    assert_eq!(session.state.runtime.run_usage.formula_executions, 1);
    for cell in ["A2", "B2", "B3"] {
        assert!(
            !session
                .state
                .runtime
                .memo
                .cache
                .contains_key(&(0, cell.parse().unwrap()))
        );
    }
    drop(session);
    let cells: Vec<_> = reads
        .0
        .into_iter()
        .filter_map(|event| match event.read {
            DependencyRead::Cell(target) => Some(target),
            _ => None,
        })
        .collect();
    assert_eq!(
        cells,
        ["A1", "B1"].map(|cell| EvaluationTarget {
            sheet: 0,
            address: cell.parse().unwrap(),
        })
    );
}

#[test]
fn real_read_and_array_limits_escape_iferror() {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    // Proven absence now skips memo/read work; retain a genuine stored-read refusal.
    for row in 0..100 {
        w.sheet_mut(0)
            .unwrap()
            .set_value(CellAddress { row, col: 0 }, 1.)
            .unwrap();
    }
    for (formula, budgets, kind) in [
        (
            "IFERROR(MATCH(7,A:A,0),99)",
            EvaluationBudgets {
                cell_work: 100,
                ..EvaluationBudgets::default()
            },
            BudgetKind::CellWork,
        ),
        (
            "IFERROR(MATCH(1,{1;2;3},0),99)",
            EvaluationBudgets {
                materialized_values: 2,
                ..EvaluationBudgets::default()
            },
            BudgetKind::Materialization,
        ),
    ] {
        let outcome = run_formula(
            &w,
            0,
            "Z20".parse().unwrap(),
            formula,
            EvaluationInputs::default(),
            budgets,
        );
        let usage = outcome.usage();
        if kind == BudgetKind::CellWork {
            // Operational refusals must escape IFERROR regardless of the
            // sparse traversal algorithm's page/row counter granularity.
            assert_eq!(usage.expressions, 7);
            // Cold sheet classification may exhaust the allowance before
            // the first candidate comparison, and remains an operational failure.
            assert!(usage.sparse_entries > 0);
            assert_eq!(usage.text_bytes, 26);
        } else {
            assert_eq!(usage.work_units, 35);
            assert_eq!(usage.expressions, 5);
            assert_eq!(usage.cell_reads, 0);
            assert_eq!(usage.sparse_entries, 0);
            assert_eq!(usage.materialized_values, 0);
            assert_eq!(usage.comparisons, 0);
            assert_eq!(usage.text_bytes, 30);
            assert_eq!(usage.completed_cells, 0);
        }
        let EvaluationOutcome::Aborted {
            failure: EvaluationError::Budget(failure),
            ..
        } = outcome
        else {
            panic!("{formula}: {outcome:?}")
        };
        assert_eq!(failure.kind, kind, "{formula}");
        assert_eq!(
            failure.limit,
            if kind == BudgetKind::CellWork { 100 } else { 2 }
        );
        assert!(failure.consumed <= failure.limit);
        assert!(failure.consumed.saturating_add(failure.requested) > failure.limit);
        assert_eq!(
            failure.consumed,
            if kind == BudgetKind::CellWork { 100 } else { 0 }
        );
        assert_eq!(
            failure.requested,
            if kind == BudgetKind::CellWork { 1 } else { 3 }
        );
        // Sparse pages let more candidate reads fit than row-wise queries;
        // refusal can occur at either a candidate read or its consumer.
        // Dense array admission precedes reads and belongs to the consumer.
        let target = failure.target.expect("failure must identify its cell");
        assert_eq!(target.sheet, 0);
        if kind == BudgetKind::CellWork {
            assert!(target.address.col == 0 || target.address == "Z20".parse().unwrap());
        } else {
            assert_eq!(target.address, "Z20".parse().unwrap());
        }
    }
}

#[test]
fn typed_selection_preserves_duplicates_type_filtering_and_wildcards() {
    let key = Number(2.);
    let ascending = vec![Number(1.), Number(2.), Number(2.), Number(3.)];
    assert_eq!(
        (select_match(&key, &ascending, SearchMode::Exact)).unwrap(),
        Some(SearchSelection { index: 1 })
    );
    assert_eq!(
        (select_match(&key, &ascending, SearchMode::Ascending)).unwrap(),
        Some(SearchSelection { index: 2 })
    );
    let descending = vec![Number(3.), Number(2.), Number(2.), Number(1.)];
    assert_eq!(
        (select_match(&key, &descending, SearchMode::Descending)).unwrap(),
        Some(SearchSelection { index: 1 })
    );
    let mixed = vec![Text("2".into()), Error(NA), Number(2.)];
    assert_eq!(
        (select_match(&key, &mixed, SearchMode::Exact)).unwrap(),
        Some(SearchSelection { index: 2 })
    );
    assert_eq!(
        (select_match(
            &Text("a*".into()),
            &[Text("ALPHA".into())],
            SearchMode::Exact
        ))
        .unwrap(),
        Some(SearchSelection { index: 0 })
    );
    assert_eq!(
        (select_match(&Number(9.), &ascending, SearchMode::Exact)).unwrap(),
        None
    );
    assert_eq!(SearchMode::from_number(0.5), SearchMode::Ascending);
    assert_eq!(SearchMode::from_number(-0.5), SearchMode::Descending);
}

#[test]
fn bound_search_modes_and_error_order_remain_runtime_coercions() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    let at = CellAddress { row: 0, col: 0 };
    for (formula, expected) in [
        ("MATCH(2,{1;2;2;3},0)", Number(2.)),
        ("MATCH(2,{1;2;2;3},0.5)", Number(3.)),
        ("MATCH(2,{3;2;2;1},-0.5)", Number(2.)),
        ("MATCH(2,{\"2\";2},0)", Number(2.)),
        ("MATCH(\"a\",{\"A\";\"a\"},0)", Number(1.)),
        ("MATCH(8,{1;2},0)", Error(NA)),
        ("VLOOKUP(2,{1,10;2,20;2,21},2,FALSE)", Number(20.)),
        ("VLOOKUP(2,{1,10;2,20;2,21},2,TRUE)", Number(21.)),
        ("HLOOKUP(2,{1,2,2;10,20,21},2,FALSE)", Number(20.)),
        ("HLOOKUP(2,{1,2,2;10,20,21},2,TRUE)", Number(21.)),
        ("MATCH(#REF!,#N/A,#DIV/0!)", Error(Ref)),
        ("MATCH(1,{1},\"bad\")", Error(Ref)),
        ("VLOOKUP(1,#N/A,#DIV/0!)", Error(VALUE_ERROR)),
        ("VLOOKUP(#REF!,#N/A)", Error(VALUE_ERROR)),
    ] {
        let syntax = poirot_parsing::formula::parse_formula(formula).unwrap();
        let plan = compile::bound::FormulaPlan::bind(&syntax).unwrap();
        assert_eq!(
            match crate::eval::entry::run_plan(
                &workbook,
                0,
                at,
                &plan,
                Default::default(),
                Default::default()
            ) {
                crate::eval::limits::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{formula}"
        );
    }
}
