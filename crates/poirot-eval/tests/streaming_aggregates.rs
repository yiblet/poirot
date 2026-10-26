//! Ordered aggregates use fixed scratch after origin-sensitive operand admission.
//! Independent arithmetic follows the named reductions and Microsoft's numeric
//! fidelity policy (subnormal final results become zero); no expected value is
//! taken from another Poirot execution.
//! https://learn.microsoft.com/en-us/troubleshoot/microsoft-365-apps/excel/floating-point-arithmetic-inaccurate-result
use poirot_eval::*;
use poirot_model::{CellError, CellValue, Workbook};

fn result(
    book: &Workbook,
    formula: &str,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula(
        book,
        0,
        "Z1".parse().unwrap(),
        formula,
        Default::default(),
        budgets,
    )
}

#[test]
fn numeric_and_logical_reductions_preserve_origins_and_empty_contracts() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (address, value) in [
        ("A1", CellValue::Number(2.)),
        ("A2", CellValue::Number(4.)),
        ("A3", CellValue::Boolean(true)),
        ("A4", CellValue::Text("8".into())),
    ] {
        sheet.set_value(address.parse().unwrap(), value).unwrap();
    }
    for (formula, expected) in [
        ("SUM(A1:A5)", CellValue::Number(6.)),
        ("SUMSQ(A1:A5)", CellValue::Number(20.)),
        ("AVERAGE(A1:A5)", CellValue::Number(3.)),
        ("MIN(A1:A5)", CellValue::Number(2.)),
        ("MAX(A1:A5)", CellValue::Number(4.)),
        ("PRODUCT(A1:A5)", CellValue::Number(8.)),
        ("SUMSQ(TRUE,\"3\",2)", CellValue::Number(14.)),
        ("PRODUCT({2,TRUE,\"3\",4})", CellValue::Number(8.)),
        ("AVERAGE(INDEX(A1:A5,1),4)", CellValue::Number(3.)),
        ("MIN(INDEX({2,4},1,2),3)", CellValue::Number(3.)),
        ("SUM(,2)", CellValue::Number(2.)),
        ("AND(A1:A5)", CellValue::Boolean(true)),
        ("OR(A1:A5)", CellValue::Boolean(true)),
        ("AND(FALSE,#REF!)", CellValue::Error(CellError::Ref)),
        ("OR(TRUE,#REF!)", CellValue::Error(CellError::Ref)),
        ("SUM(B1:B2)", CellValue::Number(0.)),
        ("SUMSQ(B1:B2)", CellValue::Number(-0.)),
        ("PRODUCT(B1:B2)", CellValue::Number(0.)),
        ("MIN(B1:B2)", CellValue::Number(0.)),
        ("MAX(B1:B2)", CellValue::Number(0.)),
        ("AVERAGE(B1:B2)", CellValue::Error(CellError::Div0)),
        ("AND(B1:B2)", CellValue::Error(CellError::Value)),
        ("OR(B1:B2)", CellValue::Error(CellError::Value)),
        ("COUNT(A1:A5)", CellValue::Number(2.)),
        ("COUNTA(A1:A5)", CellValue::Number(4.)),
        ("SUMSQ(1E-160)", CellValue::Number(0.)),
        ("PRODUCT(1E308,2)", CellValue::Error(CellError::Num)),
        ("SUM({1E16,1,-1E16,2})", CellValue::Number(2.)),
    ] {
        let EvaluationOutcome::Complete { value, .. } = result(&book, formula, Default::default())
        else {
            panic!("unexpected failure: {formula}");
        };
        assert_eq!(value, expected, "{formula}");
    }
}

#[test]
fn eager_arguments_finish_bodies_but_sheet_spans_and_later_arguments_remain_ordered() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_value("A1".parse().unwrap(), CellValue::Error(CellError::Div0))
        .unwrap();
    sheet
        .set_formula("A2".parse().unwrap(), "SUM(U:T!B1)")
        .unwrap();
    book.add_sheet("T")
        .unwrap()
        .set_formula("A1".parse().unwrap(), "SUM(U:T!B1)")
        .unwrap();
    book.add_sheet("U").unwrap();
    for function in [
        "SUM", "SUMSQ", "AVERAGE", "MIN", "MAX", "PRODUCT", "AND", "OR",
    ] {
        assert!(
            matches!(
                result(&book, &format!("{function}(A1:A2)"), Default::default()),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::UnsupportedCapability { .. },
                    ..
                }
            ),
            "{function}"
        );
        for formula in [
            format!("{function}(S:T!A1)"),
            format!("{function}(A1,T!A1)"),
        ] {
            assert!(
                matches!(
                    result(&book, &formula, Default::default()),
                    EvaluationOutcome::Complete {
                        value: CellValue::Error(CellError::Div0),
                        ..
                    }
                ),
                "{formula}"
            );
        }
    }
}

#[test]
fn fixed_scratch_does_not_accumulate_scalar_materialization() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    for function in [
        "SUM", "SUMSQ", "AVERAGE", "MIN", "MAX", "PRODUCT", "AND", "OR",
    ] {
        let formula = format!("{function}(1,1,1,1)");
        assert!(
            matches!(
                result(
                    &book,
                    &formula,
                    EvaluationBudgets {
                        materialized_values: 1,
                        ..Default::default()
                    }
                ),
                EvaluationOutcome::Complete { .. }
            ),
            "{formula}"
        );
    }
    // MEDIAN still owns a growing numeric vector, unlike the fixed reductions.
    assert!(matches!(
        result(
            &book,
            "MEDIAN(1,2,3)",
            EvaluationBudgets {
                materialized_values: 1,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(BudgetFailure {
                kind: BudgetKind::Materialization,
                requested: 2,
                ..
            }),
            ..
        }
    ));
}

/// Capture complete values at the shared cell boundary before reduction.
#[derive(Default)]
struct Observed(Vec<(EvaluationTarget, Result<CellValue, EvaluationFailure>)>);
impl CellObserver for Observed {
    fn completed(&mut self, event: CellCompletion<'_>) -> Result<(), EvaluationFailure> {
        self.0
            .push((event.target, event.result.cloned().map_err(Clone::clone)));
        Ok(())
    }
}

#[test]
fn ignored_text_keeps_full_observations_and_cold_text_failures() {
    for text in [
        CellValue::Text("x".repeat(32)),
        CellValue::TextUtf16(poirot_model::ExcelText::from_units(vec![0xd800; 16])),
    ] {
        for function in [
            "SUM", "SUMSQ", "AVERAGE", "MIN", "MAX", "PRODUCT", "AND", "OR",
        ] {
            let mut book = Workbook::new();
            let sheet = book.add_sheet("S").unwrap();
            sheet
                .set_value("A1".parse().unwrap(), text.clone())
                .unwrap();
            sheet.set_value("A2".parse().unwrap(), 1.).unwrap();
            sheet
                .set_formula("Z1".parse().unwrap(), format!("{function}(A1:A2)"))
                .unwrap();
            let policy = EvaluationPolicy::default();
            let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
            for text_bytes in [usize::MAX, 16] {
                let mut observed = Observed::default();
                let mut run = EvaluationRun::new(
                    WorkbookView::new(&book, &plan, &policy).unwrap(),
                    Default::default(),
                    EvaluationBudgets {
                        text_bytes,
                        ..Default::default()
                    },
                )
                .unwrap()
                .with_cell_observer(&mut observed);
                let result = run.evaluate_cell(EvaluationTarget {
                    sheet: 0,
                    address: "Z1".parse().unwrap(),
                });
                drop(run);
                if text_bytes == 16 {
                    assert!(
                        matches!(result, Err(EvaluationFailure::Budget(BudgetFailure { kind: BudgetKind::TextBytes, target: Some(EvaluationTarget { address, .. }), .. })) if address == "A1".parse().unwrap()),
                        "{function}"
                    );
                    assert_eq!(observed.0.len(), 2);
                } else {
                    assert_eq!(
                        result,
                        Ok(if function == "AND" || function == "OR" {
                            CellValue::Boolean(true)
                        } else {
                            CellValue::Number(1.)
                        })
                    );
                    assert_eq!(observed.0.len(), 3);
                    assert_eq!(observed.0.first().unwrap().1, Ok(text.clone()));
                    assert_eq!(
                        observed
                            .0
                            .iter()
                            .map(|(target, _)| target.address)
                            .collect::<Vec<_>>(),
                        ["A1", "A2", "Z1"].map(|address| address.parse().unwrap())
                    );
                }
            }
        }
    }
}

#[test]
fn eager_multi_area_reductions_finish_later_areas_before_reporting_errors() {
    // The evaluator's eager operand contract includes every area of one union.
    // An operational failure from its second area must beat a first-area #DIV/0!.
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_value("A1".parse().unwrap(), CellValue::Error(CellError::Div0))
        .unwrap();
    sheet
        .set_formula("B1".parse().unwrap(), "SUM(U:T!A1)")
        .unwrap();
    book.add_sheet("T").unwrap();
    book.add_sheet("U").unwrap();
    for function in ["SUMSQ", "AVERAGE", "MIN", "MAX", "PRODUCT"] {
        assert!(
            matches!(
                result(&book, &format!("{function}((A1,B1))"), Default::default()),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::UnsupportedCapability { .. },
                    ..
                }
            ),
            "{function}"
        );
    }
}

#[test]
fn eager_numeric_reductions_preserve_special_number_and_error_order() {
    // Non-SUM handlers currently reject raw nonfinite inputs as an unsupported
    // capability. This pins that compatibility gap and ordered failure policy;
    // it does not claim Excel reports an operational error for these values.
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (address, value) in [
        ("A1", CellValue::Number(f64::INFINITY)),
        ("B1", CellValue::Error(CellError::Div0)),
        ("A2", CellValue::Error(CellError::Div0)),
        ("B2", CellValue::Number(f64::INFINITY)),
    ] {
        sheet.set_value(address.parse().unwrap(), value).unwrap();
    }
    for function in ["SUMSQ", "AVERAGE", "MIN", "MAX", "PRODUCT"] {
        for operand in ["A1:B1", "CHOOSE({1,2},A1,B1)"] {
            assert!(
                matches!(
                    result(&book, &format!("{function}({operand})"), Default::default()),
                    EvaluationOutcome::Aborted {
                        failure: EvaluationFailure::UnsupportedCapability { .. },
                        ..
                    }
                ),
                "{function}({operand})"
            );
        }
        for operand in ["A2:B2", "CHOOSE({1,2},A2,B2)"] {
            assert!(
                matches!(
                    result(&book, &format!("{function}({operand})"), Default::default()),
                    EvaluationOutcome::Complete {
                        value: CellValue::Error(CellError::Div0),
                        ..
                    }
                ),
                "{function}({operand})"
            );
        }
    }
}
