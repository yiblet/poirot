//! Modern grid/default contracts follow Microsoft's OFFSET and worksheet-limit docs:
//! https://support.microsoft.com/en-us/excel/functions/offset-function
//! https://support.microsoft.com/en-gb/excel/excel-specifications-and-limits
use poirot::eval::{EvaluationBudgets, EvaluationFailure, EvaluationOutcome, run_formula};
use poirot::model::{CellError, CellValue, Workbook};

fn book() -> Workbook {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    book.add_sheet("Other").unwrap();
    book.sheet_mut(1)
        .unwrap()
        .set_value("B6".parse().unwrap(), 42.)
        .unwrap();
    book
}

fn outcome(
    book: &Workbook,
    formula: &str,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula(
        book,
        0,
        "Z20".parse().unwrap(),
        formula,
        Default::default(),
        budgets,
    )
}

fn value(book: &Workbook, formula: &str) -> CellValue {
    match outcome(book, formula, Default::default()) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{formula}: {other:?}"),
    }
}

#[test]
fn offset_retains_sheet_identity_defaults_and_modern_large_coordinates() {
    let book = book();
    for (formula, expected) in [
        ("OFFSET(Other!D3,3,-2,1,1)", 42.),
        ("ROWS(OFFSET(D3:F5,3,-2))", 3.),
        ("COLUMNS(OFFSET(D3:F5,3,-2,,))", 3.),
        ("ROW(OFFSET(A1,32768,0))", 32769.),
        ("ROW(OFFSET(A1,65536,0))", 65537.),
        ("ROW(OFFSET(A1,1048575,0))", 1048576.),
        ("COLUMN(OFFSET(A1,0,16383))", 16384.),
        ("ROWS(OFFSET(A1,0,0,1048576,16384))", 1048576.),
        ("ROW(OFFSET(D5,,))", 5.),
    ] {
        assert_eq!(
            value(&book, formula),
            CellValue::Number(expected),
            "{formula}"
        );
    }
}

#[test]
fn offset_rejects_grid_overflow_and_preserves_spreadsheet_errors() {
    let book = book();
    for (formula, expected) in [
        ("OFFSET(D3,-3,-3)", CellError::Ref),
        ("OFFSET(A1,1048576,0)", CellError::Ref),
        ("OFFSET(A1,0,16384)", CellError::Ref),
        ("OFFSET(A1048576,0,0,2,1)", CellError::Ref),
        ("OFFSET(XFD1,0,0,1,2)", CellError::Ref),
        ("OFFSET(A1,1E100,0)", CellError::Ref),
        ("OFFSET(A1,0,0,0,1)", CellError::Ref),
        ("OFFSET(A1,0,0,Z1,1)", CellError::Ref),
        ("OFFSET(1,0,0)", CellError::Value),
        ("OFFSET((A1,B1),0,0)", CellError::Value),
        ("OFFSET(A1)", CellError::Value),
        ("OFFSET(A1,0)", CellError::Value),
        ("OFFSET(A1,1/0,0)", CellError::Div0),
        ("OFFSET(#N/A,1/0,0)", CellError::NA),
    ] {
        assert_eq!(
            value(&book, formula),
            CellValue::Error(expected),
            "{formula}"
        );
    }
}

#[test]
fn offset_compatibility_rules_are_distinct_from_native_excel_evidence() {
    let book = book();
    // Fractions corroborated in LibreOffice 26.2.6.3; native Excel unverified.
    // Negative sizes are retained normalized compatibility behavior; Calc rejects them.
    for (formula, expected) in [
        ("ROW(OFFSET(D5,-0.9,0))", 5.),
        ("ROW(OFFSET(D5,0.9,0))", 5.),
        ("ROWS(OFFSET(D5,0,0,1.9,1))", 1.),
        ("ROWS(OFFSET(D5,0,0,-2,-2))", 2.),
        ("ROW(INDEX(OFFSET(D5,0,0,-2,-2),1,1))", 4.),
        ("COLUMN(INDEX(OFFSET(D5,0,0,-2,-2),1,1))", 3.),
    ] {
        assert_eq!(
            value(&book, formula),
            CellValue::Number(expected),
            "{formula}"
        );
    }
}

#[test]
fn offset_geometry_reads_no_cells_and_operational_failures_escape_masks() {
    let book = book();
    let result = outcome(
        &book,
        "ROWS(OFFSET(A1,0,0,1048576,16384))",
        EvaluationBudgets {
            materialized_values: 2,
            ..Default::default()
        },
    );
    match result {
        EvaluationOutcome::Complete { value, usage } => {
            assert_eq!(value, CellValue::Number(1048576.));
            assert_eq!(usage.cell_reads, 0);
        }
        other => panic!("{other:?}"),
    }
    for (formula, limit) in [
        ("IFERROR(ROWS(OFFSET(A1,0,0)),9)", 0),
        ("IFERROR(OFFSET(A1,SEQUENCE(20),0),9)", 5),
    ] {
        assert!(
            matches!(
                outcome(
                    &book,
                    formula,
                    EvaluationBudgets {
                        materialized_values: limit,
                        ..Default::default()
                    }
                ),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::Budget(_),
                    ..
                }
            ),
            "{formula}"
        );
    }
    // Source and earlier control errors stop before later control demand.
    assert!(matches!(
        outcome(
            &book,
            "IFERROR(OFFSET(#REF!,SEQUENCE(20),0),9)",
            EvaluationBudgets {
                materialized_values: 5,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Complete {
            value: CellValue::Number(9.),
            ..
        }
    ));
    assert!(matches!(
        outcome(
            &book,
            "IFERROR(OFFSET(A1,#REF!,SEQUENCE(20)),9)",
            EvaluationBudgets {
                materialized_values: 5,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Complete {
            value: CellValue::Number(9.),
            ..
        }
    ));
    assert_eq!(
        value(&book, "OFFSET(A1,0,0,#N/A,#DIV/0!)"),
        CellValue::Error(CellError::Div0)
    );
    assert_eq!(
        value(&book, "IF(FALSE,OFFSET(A1,1/0,0),7)"),
        CellValue::Number(7.)
    );
}

#[test]
fn offset_tracks_only_consumed_cells_and_executed_volatility() {
    use poirot::eval::{
        DependencyRead, DependencyReadSink, EvaluationRun, EvaluationTarget, FormulaCompiler,
        PreparationPolicy, ReadEvent, WorkbookView,
        registry::{FunctionId, ReferenceFunction},
    };
    /// Caller-owned observations of actual reference and control consumption.
    #[derive(Default)]
    struct Reads(Vec<ReadEvent>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event);
        }
    }
    let mut book = book();
    let sheet = book.sheet_mut(0).unwrap();
    sheet
        .set_formula("A1".parse().unwrap(), "UNKNOWN_FUNCTION()")
        .unwrap();
    sheet.set_value("B1".parse().unwrap(), 7.).unwrap();
    sheet.set_value("C1".parse().unwrap(), 1.).unwrap();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
    for (formula, executed) in [
        ("SUM(OFFSET(A1,0,C1))", true),
        ("IF(FALSE,OFFSET(A1,0,C1),7)", false),
    ] {
        let formula = FormulaCompiler::new().compile_formula(formula, &policy);
        let mut reads = Reads::default();
        let mut run = EvaluationRun::new(
            WorkbookView::new(&book, &plan, &policy).unwrap(),
            Default::default(),
            Default::default(),
        )
        .unwrap()
        .with_dependency_sink(&mut reads);
        assert_eq!(
            run.evaluate_formula(
                EvaluationTarget {
                    sheet: 0,
                    address: "Z20".parse().unwrap()
                },
                &formula
            )
            .unwrap(),
            CellValue::Number(7.)
        );
        run.finish(Ok(()));
        assert_eq!(
            reads.0.iter().any(|event| event.read
                == DependencyRead::Volatile(FunctionId::Reference(ReferenceFunction::Offset))),
            executed
        );
        for address in ["B1", "C1"] {
            assert_eq!(
                reads.0.iter().any(|event| event.read
                    == DependencyRead::Cell(EvaluationTarget {
                        sheet: 0,
                        address: address.parse().unwrap()
                    })),
                executed
            );
        }
        assert!(!reads.0.iter().any(|event| event.read
            == DependencyRead::Cell(EvaluationTarget {
                sheet: 0,
                address: "A1".parse().unwrap()
            })));
    }
}
