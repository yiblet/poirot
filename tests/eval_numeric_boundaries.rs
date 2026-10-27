//! Ordinary summation and numeric-domain errors at the public evaluator boundary.
use poirot::calculation::{
    EvaluationBudgets, EvaluationError, EvaluationInputs, EvaluationOutcome, run_formula,
};
use poirot::{CellError, CellValue, Workbook};

fn evaluate(
    workbook: &Workbook,
    formula: &str,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula(
        workbook,
        0,
        "D10".parse().unwrap(),
        formula,
        EvaluationInputs::default(),
        budgets,
    )
}

#[test]
fn sum_visits_each_reference_union_member() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    for (address, number) in [("A1", 2.), ("A2", 3.), ("B1", 7.)] {
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value(address, number)
            .unwrap();
    }
    // Each union member contributes its numbers: 2 + 3 + 7 = 12.
    for formula in [
        "SUM((A1:A2,B1))",
        "SUM((OFFSET(A1,0,0,2,1),B1))",
        "IFERROR(SUM((OFFSET(A1,0,0,2,1),B1)),99)",
    ] {
        assert!(
            matches!(
                evaluate(&workbook, formula, Default::default()),
                EvaluationOutcome::Complete {
                    value: CellValue::Number(12.),
                    ..
                }
            ),
            "{formula}"
        );
    }
}

#[test]
fn nonfinite_comparisons_use_catchable_numeric_domain_errors() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    // Nonfinite inputs are outside the numeric kernel's domain, as in A1+0.
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value("A1", number)
            .unwrap();
        for formula in ["A1=0", "0<A1", "A1=A1", "A1+0"] {
            assert!(
                matches!(
                    evaluate(&workbook, formula, Default::default()),
                    EvaluationOutcome::Complete {
                        value: CellValue::Error(CellError::Num),
                        ..
                    }
                ),
                "{formula}"
            );
        }
        assert!(matches!(
            evaluate(&workbook, "IFERROR(A1=0,42)", Default::default()),
            EvaluationOutcome::Complete {
                value: CellValue::Number(42.),
                ..
            }
        ));
    }
    // Formula handlers still cannot catch an exhausted evaluator work budget.
    assert!(matches!(
        evaluate(
            &workbook,
            "IFERROR(A1=0,42)",
            EvaluationBudgets {
                run_work: 0,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationError::Budget(_),
            ..
        }
    ));
}
