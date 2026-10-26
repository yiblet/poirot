use poirot::eval::{
    BudgetFailure, BudgetKind, EvaluationBudgets, EvaluationFailure, EvaluationInputs,
    EvaluationOutcome, run_formula,
};
use poirot::model::{CellAddress, CellError, CellValue, Workbook};
const AT: CellAddress = CellAddress { row: 4, col: 4 };
fn book(names: &[&str]) -> Workbook {
    let mut book = Workbook::new();
    for (i, name) in names.iter().enumerate() {
        book.add_sheet(*name).unwrap();
        book.sheet_mut(i)
            .unwrap()
            .set_value("A1".parse().unwrap(), (i + 1) as f64)
            .unwrap();
        book.sheet_mut(i)
            .unwrap()
            .set_value("B2".parse().unwrap(), (i + 1) as f64 * 10.)
            .unwrap();
    }
    book
}
#[test]
fn aggregates_traverse_three_real_sheets_and_bounded_rectangles() {
    let book = book(&["Sheet1", "Sheet2", "Sheet3"]);
    for (formula, number) in [
        ("SUM(Sheet1:Sheet3!A1)", 6.),
        ("SUM(Sheet1:Sheet3!A1:B2)", 66.),
        ("SUM(Sheet1:Sheet3!B2:A1)", 66.),
        ("AVERAGE(Sheet1:Sheet3!A1)", 2.),
        ("COUNT(Sheet1:Sheet3!A1:B2)", 6.),
        ("SUM(Sheet2:Sheet2!A1)", 2.),
        ("SUM(Sheet1:Sheet3!A1,TRUE,\"4\")", 11.),
    ] {
        assert_eq!(
            match poirot::eval::run_formula(
                &book,
                0,
                AT,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Number(number),
            "{formula}"
        );
    }
}
#[test]
fn quoted_spans_and_sheet_topology_select_ordered_membership() {
    let first = book(&["O'Brien", "Middle", "Last Data", "Outside"]);
    assert_eq!(
        match poirot::eval::run_formula(
            &first,
            0,
            AT,
            "SUM('O''Brien:Last Data'!A1)",
            Default::default(),
            Default::default()
        ) {
            poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(6.)
    );
    let changed = book(&["O'Brien", "Outside", "Middle", "Last Data"]);
    assert_eq!(
        match poirot::eval::run_formula(
            &changed,
            0,
            AT,
            "SUM('O''Brien:Last Data'!A1)",
            Default::default(),
            Default::default()
        ) {
            poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(10.)
    );
}
#[test]
fn referenced_coercion_and_error_precedence_remain_sheet_then_row_then_column() {
    let mut book = book(&["Sheet1", "Sheet2", "Sheet3"]);
    book.sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), CellValue::Text("99".into()))
        .unwrap();
    book.sheet_mut(1)
        .unwrap()
        .set_value("A1".parse().unwrap(), true)
        .unwrap();
    assert_eq!(
        match poirot::eval::run_formula(
            &book,
            0,
            AT,
            "SUM(Sheet1:Sheet3!A1)",
            Default::default(),
            Default::default()
        ) {
            poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(3.)
    );
    book.sheet_mut(0)
        .unwrap()
        .set_value("B1".parse().unwrap(), CellValue::Error(CellError::Div0))
        .unwrap();
    book.sheet_mut(1)
        .unwrap()
        .set_value("A1".parse().unwrap(), CellValue::Error(CellError::Num))
        .unwrap();
    assert_eq!(
        match poirot::eval::run_formula(
            &book,
            0,
            AT,
            "SUM(Sheet1:Sheet3!A1:B2)",
            Default::default(),
            Default::default()
        ) {
            poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Div0)
    );
    assert_eq!(
        match poirot::eval::run_formula(
            &book,
            0,
            AT,
            "SUM(Sheet2:Sheet3!A1:B2)",
            Default::default(),
            Default::default()
        ) {
            poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Num)
    );
}
#[test]
fn native_endpoint_failures_stay_operational() {
    let book = book(&["Sheet1", "Sheet2", "Sheet3"]);
    // Microsoft limits 3-D references to supported collection consumers;
    // scalar FACT's former first-sheet selection is no longer a contract.
    // https://support.microsoft.com/en-us/excel/overview-of-formulas-in-excel-for-the-web
    for formula in [
        "SUM(Sheet1:Missing!A1)",
        "IFERROR(SUM(Sheet1:Missing!A1),99)",
    ] {
        assert!(matches!(
            run_formula(
                &book,
                0,
                AT,
                formula,
                EvaluationInputs::default(),
                EvaluationBudgets::default()
            ),
            EvaluationOutcome::Aborted {
                failure: EvaluationFailure::UnsupportedCapability { .. },
                ..
            }
        ));
    }
    assert!(matches!(
        run_formula(
            &book,
            0,
            AT,
            "SUM(Sheet3:Sheet1!A1)",
            EvaluationInputs::default(),
            EvaluationBudgets::default()
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::UnsupportedCapability { .. },
            ..
        }
    ));
}
#[test]
fn native_missing_first_endpoint_selects_workbook_first_sheet_alone() {
    // Retained authenticated HSSF probe: raw SUM(#REF!A1), numeric result 1.
    // A different caller sheet and a surviving last endpoint distinguish the
    // observed singleton fallback from caller-sheet or remaining-span guesses.
    let book = book(&["Sheet1", "Sheet2", "Sheet3"]);
    for caller in [0, 2] {
        assert_eq!(
            match poirot::eval::run_formula(
                &book,
                caller,
                AT,
                "SUM(Missing:Sheet3!A1)",
                Default::default(),
                Default::default()
            ) {
                poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Number(1.)
        );
    }
    assert_eq!(
        match poirot::eval::run_formula(
            &book,
            2,
            AT,
            "IFERROR(SUM(Missing:Sheet3!A1),99)",
            Default::default(),
            Default::default()
        ) {
            poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(1.)
    );
}
#[test]
fn span_allocation_and_work_limits_are_operational_even_under_iferror() {
    let book = book(&["Sheet1", "Sheet2", "Sheet3"]);
    assert!(matches!(
        run_formula(
            &book,
            0,
            AT,
            "IFERROR(SUM(Sheet1:Sheet3!A1),0)",
            EvaluationInputs::default(),
            EvaluationBudgets {
                materialized_values: 2,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(BudgetFailure {
                kind: BudgetKind::Materialization,
                ..
            }),
            ..
        }
    ));
    assert!(matches!(
        run_formula(
            &book,
            0,
            AT,
            "SUM(Sheet1:Sheet3!A1)",
            EvaluationInputs::default(),
            EvaluationBudgets {
                run_work: 1,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(BudgetFailure {
                kind: BudgetKind::RunWork,
                ..
            }),
            ..
        }
    ));
}
#[test]
fn earlier_sheet_error_prevents_evaluating_later_operational_failure() {
    let mut book = book(&["Sheet1", "Sheet2", "Sheet3"]);
    book.sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), CellValue::Error(CellError::Div0))
        .unwrap();
    book.sheet_mut(2)
        .unwrap()
        .set_formula_with_cached_value(
            "A1".parse().unwrap(),
            "SUM(Sheet3:Sheet1!B1)",
            CellValue::Number(-999.),
        )
        .unwrap();
    assert!(matches!(
        run_formula(
            &book,
            0,
            AT,
            "SUM(Sheet1:Sheet3!A1)",
            EvaluationInputs::default(),
            EvaluationBudgets::default()
        ),
        EvaluationOutcome::Complete {
            value: CellValue::Error(CellError::Div0),
            ..
        }
    ));
}

#[test]
fn prepared_span_metadata_retains_ordered_endpoints_and_anchors() {
    use poirot::eval::compiler::{PotentialReference, PreparedFormula};
    let prepared = poirot::eval::FormulaCompiler::default().compile_formula(
        "IF(FALSE,SUM(Sheet1:Sheet3!$A1:B$2),7)",
        &poirot::eval::PreparationPolicy::default(),
    );
    let PreparedFormula::Ready(compiled) = prepared.as_ref() else {
        panic!("span not compiled")
    };
    let [PotentialReference::SheetSpan(span)] =
        compiled.potential_dependencies().references.as_slice()
    else {
        panic!("typed span metadata missing")
    };
    assert_eq!(
        (&*span.first_sheet, &*span.last_sheet),
        ("Sheet1", "Sheet3")
    );
    assert!(span.start.column_absolute);
    assert!(!span.start.row_absolute);
    assert!(span.end.as_ref().unwrap().row_absolute);
}

#[test]
fn selected_span_consumes_ordered_layout_and_endpoints_but_lazy_span_does_not() {
    use poirot::eval::run::{
        DependencyRead, DependencyReadSink, EvaluationRun, ReadEvent, WorkbookView,
    };
    #[derive(Default)]
    /// Records sheet layout and endpoint reads from selected span expressions.
    struct Reads(Vec<DependencyRead>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event.read);
        }
    }
    let book = book(&["Sheet1", "Sheet2", "Sheet3"]);
    let policy = poirot::eval::PreparationPolicy::default();
    let mut compiler = poirot::eval::FormulaCompiler::default();
    let plan = compiler.compile_workbook(&book, &policy);
    for (formula, selected) in [
        ("SUM(Sheet1:Sheet3!A1)", true),
        ("IF(FALSE,SUM(Sheet1:Sheet3!A1),7)", false),
    ] {
        let prepared = compiler.compile_formula(formula, &policy);
        let mut reads = Reads::default();
        {
            let mut run = EvaluationRun::new(
                WorkbookView::new(&book, &plan, &policy).unwrap(),
                EvaluationInputs::default(),
                EvaluationBudgets::default(),
            )
            .unwrap()
            .with_dependency_sink(&mut reads);
            run.evaluate_formula(
                poirot::eval::EvaluationTarget {
                    sheet: 0,
                    address: AT,
                },
                &prepared,
            )
            .unwrap();
        }
        let namespace: Vec<_> = reads
            .0
            .into_iter()
            .filter(|read| {
                matches!(
                    read,
                    DependencyRead::SheetLayout | DependencyRead::Sheet { .. }
                )
            })
            .collect();
        let expected = if selected {
            vec![
                DependencyRead::SheetLayout,
                DependencyRead::Sheet {
                    current: 0,
                    name: "Sheet1".into(),
                    selected: Some(0),
                },
                DependencyRead::Sheet {
                    current: 0,
                    name: "Sheet3".into(),
                    selected: Some(2),
                },
            ]
        } else {
            vec![]
        };
        assert_eq!(namespace, expected, "{formula}");
    }
}
