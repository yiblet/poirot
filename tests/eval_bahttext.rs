//! BAHTTEXT's documented example plus independently authored currency cases.
//! https://support.microsoft.com/en-us/excel/functions/bahttext-function
//! Additional cases test Poirot's chosen Thai wording and rounding rules; their
//! Excel parity is unverified. For 101, the final one is written as หนึ่ง, not เอ็ด.
use poirot::eval::{
    EvaluationBudgets, EvaluationOutcome, RunConfiguration, run_formula_with_configuration,
};
use poirot::model::{CellAddress, CellError, CellValue, Workbook};

fn evaluate(formula: &str, budgets: EvaluationBudgets) -> EvaluationOutcome<CellValue> {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    run_formula_with_configuration(
        &book,
        0,
        CellAddress { row: 0, col: 10 },
        formula,
        &RunConfiguration::default(),
        budgets,
    )
}

fn value(formula: &str) -> CellValue {
    match evaluate(formula, Default::default()) {
        EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("{formula}: {outcome:?}"),
    }
}

#[test]
fn documented_example_and_declared_currency_rules() {
    for (formula, expected) in [
        ("BAHTTEXT(1234)", "หนึ่งพันสองร้อยสามสิบสี่บาทถ้วน"),
        ("BAHTTEXT(12.65)", "สิบสองบาทหกสิบห้าสตางค์"),
        ("BAHTTEXT(0)", "ศูนย์บาทถ้วน"),
        ("BAHTTEXT(-0.004)", "ศูนย์บาทถ้วน"),
        ("BAHTTEXT(-0.005)", "ลบหนึ่งสตางค์"),
        ("BAHTTEXT(1.005)", "หนึ่งบาทหนึ่งสตางค์"),
        ("BAHTTEXT(0.999)", "หนึ่งบาทถ้วน"),
        ("BAHTTEXT(99.999)", "หนึ่งร้อยบาทถ้วน"),
        ("BAHTTEXT(21)", "ยี่สิบเอ็ดบาทถ้วน"),
        ("BAHTTEXT(101)", "หนึ่งร้อยหนึ่งบาทถ้วน"),
        ("BAHTTEXT(111)", "หนึ่งร้อยสิบเอ็ดบาทถ้วน"),
        ("BAHTTEXT(1000000000000)", "หนึ่งล้านล้านบาทถ้วน"),
        ("BAHTTEXT(1000001)", "หนึ่งล้านหนึ่งบาทถ้วน"),
        ("BAHTTEXT(\"12.65\")", "สิบสองบาทหกสิบห้าสตางค์"),
        ("BAHTTEXT(TRUE)", "หนึ่งบาทถ้วน"),
        ("_xlfn.BAHTTEXT(1)", "หนึ่งบาทถ้วน"),
        ("BAHTTEXT(A1)", "ศูนย์บาทถ้วน"),
        ("INDEX(BAHTTEXT({1,21}),1,2)", "ยี่สิบเอ็ดบาทถ้วน"),
    ] {
        assert_eq!(
            value(formula),
            CellValue::Text(expected.into()),
            "{formula}"
        );
    }
}

#[test]
fn errors_and_arity_propagate() {
    for (formula, expected) in [
        ("BAHTTEXT()", CellError::Value),
        ("BAHTTEXT(1,2)", CellError::Value),
        ("BAHTTEXT(\"hello\")", CellError::Value),
        ("BAHTTEXT(1/0)", CellError::Div0),
        ("BAHTTEXT(#N/A)", CellError::NA),
    ] {
        assert_eq!(value(formula), CellValue::Error(expected), "{formula}");
    }
}

#[test]
fn unverified_large_domain_and_resource_refusal_are_not_spreadsheet_errors() {
    assert!(matches!(
        evaluate("IFERROR(BAHTTEXT(1E20),99)", Default::default()),
        EvaluationOutcome::Aborted {
            failure: poirot::eval::EvaluationError::UnsupportedCapability {
                reason: poirot::eval::EvaluationReasonError::BahtMagnitudeOutsideVerifiedDomain,
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        evaluate(
            "IFERROR(INDEX(BAHTTEXT({1,2}),1,1),99)",
            EvaluationBudgets {
                materialized_values: 1,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
    for budgets in [
        EvaluationBudgets {
            text_bytes: 16,
            ..Default::default()
        },
        EvaluationBudgets {
            run_work: 8,
            ..Default::default()
        },
    ] {
        let outcome = evaluate("IFERROR(BAHTTEXT(1234),99)", budgets);
        assert!(
            matches!(outcome, EvaluationOutcome::Aborted { .. }),
            "{outcome:?}"
        );
    }
}

#[test]
fn nonfinite_numeric_cells_produce_numeric_errors() {
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut book = Workbook::new();
        book.add_sheet("S").unwrap();
        book.sheet_mut(0)
            .unwrap()
            .set_value(CellAddress { row: 0, col: 0 }, CellValue::Number(number))
            .unwrap();
        let result = run_formula_with_configuration(
            &book,
            0,
            CellAddress { row: 0, col: 10 },
            "BAHTTEXT(A1)",
            &RunConfiguration::default(),
            Default::default(),
        );
        assert!(
            matches!(
                result,
                EvaluationOutcome::Complete {
                    value: CellValue::Error(CellError::Num),
                    ..
                }
            ),
            "{number}: {result:?}"
        );
    }
}
