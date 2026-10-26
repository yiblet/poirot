use poirot_eval::{
    BuiltinSemanticsPolicy, EvaluationBudgets, EvaluationFailure, EvaluationOutcome,
    RunConfiguration, run_formula_with_configuration,
};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};

fn run(formula: &str, excel: bool, budgets: EvaluationBudgets) -> EvaluationOutcome<CellValue> {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    for (address, value) in [
        ("A1", CellValue::Number(0.1)),
        ("A2", CellValue::Number(2.)),
        ("A3", CellValue::Text("100".into())),
        ("A4", CellValue::Number(-16.)),
        ("A5", CellValue::Boolean(true)),
        ("A6", CellValue::Error(CellError::NA)),
        ("A7", CellValue::Error(CellError::Ref)),
    ] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(address.parse().unwrap(), value)
            .unwrap();
    }
    let config = RunConfiguration::default().with_builtin_policy(if excel {
        BuiltinSemanticsPolicy::ExcelDocumentation
    } else {
        BuiltinSemanticsPolicy::PinnedPoi
    });
    run_formula_with_configuration(
        &w,
        0,
        CellAddress { row: 0, col: 25 },
        formula,
        &config,
        budgets,
    )
}
fn value(formula: &str, excel: bool) -> CellValue {
    match run(formula, excel, EvaluationBudgets::default()) {
        EvaluationOutcome::Complete { value, .. } => value,
        result => panic!("{formula}: {result:?}"),
    }
}
fn close(formula: &str, expected: f64) {
    match value(formula, true) {
        CellValue::Number(actual) => assert!(
            (actual - expected).abs() < 1e-10,
            "{formula}: {actual} != {expected}"
        ),
        result => panic!("{formula}: {result:?}"),
    }
}
#[test]
fn finance_zero_rate_optional_values_and_annuity_timing() {
    // At 10%, a 100 loan with two end payments of 50 leaves 16 owed.
    // End-period interest is 10 then 6; principal is 40 then 44.
    for (formula, expected) in [
        ("IPMT(0,1,2,100)", 0.),
        ("PPMT(0,2,2,100)", -50.),
        ("PPMT(0,2,2,100,20,1)", -60.),
        ("IPMT(0,2,2,100,20,1)", 0.),
        ("IPMT(0.1,1,2,100,-16)", -10.),
        ("IPMT(0.1,2,2,100,-16,0)", -6.),
        ("PPMT(0.1,2,2,100,-16,0)", -44.),
        ("IPMT(A1,1,A2,A3,A4,A5)", 0.),
        ("PPMT(A1,1,A2,A3,A4,A5)", -50. / 1.1),
        ("IPMT(A1,2,A2,A3,A4,A5)", -6. / 1.1),
        ("PPMT(A1,2,A2,A3,A4,A5)", -40.),
        ("PPMT(0,1.5,2.5,100,25,1)", -50.),
        ("IPMT(0,1,2,100,,)", 0.),
        ("PPMT(0,1,2,100,,)", -50.),
        (
            "IPMT(0.1,1.5,2.5,100)+PPMT(0.1,1.5,2.5,100)-PMT(0.1,2.5,100)",
            0.,
        ),
    ] {
        close(formula, expected);
    }
    assert_ne!(
        value("IPMT(0.1,1.5,2.5,100)", true),
        value("IPMT(0.1,1,2,100)", true)
    );
    assert_ne!(
        value("IPMT(0.1,1.5,2.5,100)", true),
        value("IPMT(0.1,1,2.5,100)", true)
    );
    assert_ne!(
        value("PPMT(0.1,1.5,2.5,100)", true),
        value("PPMT(0.1,1.5,2,100)", true)
    );
}
#[test]
fn finance_invalid_periods_types_and_ordered_errors() {
    for function in ["IPMT", "PPMT"] {
        for args in [
            "0,0,2,100",
            "0,3,2,100",
            "0,1,0,100",
            "0,1,2,100,0,2",
            "0,1,2,100,0,0.5",
        ] {
            assert_eq!(
                value(&format!("{function}({args})"), true),
                CellValue::Error(CellError::Num)
            );
        }
        assert_eq!(
            value(&format!("{function}(0,1,2,100,\"bad\")"), true),
            CellValue::Error(CellError::Value)
        );
        assert_eq!(
            value(&format!("{function}(\"bad\",1,2,100,A6)"), true),
            CellValue::Error(CellError::NA)
        );
        assert_eq!(
            value(&format!("{function}(0,1,2,100,0,A6)"), true),
            CellValue::Error(CellError::NA)
        );
        assert_eq!(
            value(
                &format!("{function}(UNKNOWN_FUNCTION(),1,2,100,0,0,0)"),
                true
            ),
            CellValue::Error(CellError::Value)
        );
    }
}
#[test]
fn finance_source_error_priority_is_member_local_and_operational_failures_escape() {
    // Independent Calc 26.8.1.1 observation: source #N/A precedes invalid
    // numeric text in IPMT/PPMT; the public contract is retained per member.
    // All eleven eager financial scalar consumers were corroborated with a
    // referenced #REF! produced by INDEX(C1,2,1), including its error control.
    for formula in [
        "FV(\"bad\",1,1,A7)",
        "PV(\"bad\",1,1,A7)",
        "PMT(\"bad\",1,1,A7)",
        "NPER(\"bad\",1,1,A7)",
        "IPMT(\"bad\",1,2,100,A7)",
        "PPMT(\"bad\",1,2,100,A7)",
        "RATE(\"bad\",1,100,A7)",
        "DOLLARDE(\"bad\",A7)",
        "DOLLARFR(\"bad\",A7)",
        "FIXED(\"bad\",A7)",
        "DOLLAR(\"bad\",A7)",
    ] {
        for excel in [true, false] {
            assert_eq!(value(formula, excel), CellValue::Error(CellError::Ref));
        }
    }
    for function in ["IPMT", "PPMT"] {
        for (column, expected) in [(1, CellError::NA), (2, CellError::Value)] {
            let formula =
                format!("INDEX({function}({{\"bad\",\"bad\"}},1,2,100,{{#N/A,0}}),1,{column})");
            for excel in [true, false] {
                assert_eq!(value(&formula, excel), CellValue::Error(expected));
            }
        }
        let formula = format!("IFERROR({function}(\"bad\",1,2,100,REPT(\"x\",32)),7)");
        for excel in [true, false] {
            assert!(matches!(
                run(
                    &formula,
                    excel,
                    EvaluationBudgets {
                        text_bytes: 4,
                        ..Default::default()
                    }
                ),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::Budget(_),
                    ..
                }
            ));
        }
    }
}
#[test]
fn modern_normal_cdf_pdf_boolean_coercion_and_references() {
    for (formula, expected) in [
        ("NORM.S.DIST(0,TRUE)", 0.5),
        ("NORM.S.DIST(0,FALSE)", 0.3989422804014327),
        ("NORM.S.DIST(-1,TRUE)", 0.15865525393145707),
        ("NORM.S.DIST(1,TRUE)", 0.8413447460685429),
        ("NORM.S.DIST(-1,FALSE)", 0.24197072451914337),
        ("NORM.S.DIST(-40,TRUE)", 0.),
        ("NORM.S.DIST(40,TRUE)", 1.),
        ("NORM.S.DIST(0,A5)", 0.5),
        ("NORM.S.DIST(0,\"false\")", 0.3989422804014327),
        ("NORM.S.DIST(0,)", 0.3989422804014327),
        ("NORM.S.DIST(A3,0)", 0.),
    ] {
        close(formula, expected);
    }
    for (formula, error) in [
        ("NORM.S.DIST(0)", CellError::Value),
        ("NORM.S.DIST(0,\"bad\")", CellError::Value),
        ("NORM.S.DIST(A6,TRUE)", CellError::NA),
        ("NORM.S.DIST(0,A6)", CellError::NA),
        ("NORM.S.DIST(UNKNOWN_FUNCTION(),TRUE,1)", CellError::Value),
    ] {
        assert_eq!(value(formula, true), CellValue::Error(error));
    }
}
#[test]
fn both_policies_use_modern_finance_and_normal_contracts() {
    // Microsoft IPMT/PPMT describe interest and principal of the annuity:
    // https://support.microsoft.com/en-us/excel/functions/ipmt-function
    // At zero interest the two payments divide 100 equally; beginning-period
    // interest is zero before any borrowing period has elapsed.
    // NORMSDIST is the standard normal CDF, including negative z:
    // https://support.microsoft.com/en-au/excel/functions/normsdist-function
    // Phi(-1) is the Gaussian integral, corroborated by Calc 26.8.1.1.
    for excel in [true, false] {
        for (formula, expected) in [
            ("IPMT(0,1,2,100)", 0.),
            ("PPMT(0,1,2,100)", -50.),
            ("IPMT(0.1,1,2,100,0,1)", 0.),
            ("NORM.S.DIST(0,TRUE)", 0.5),
            ("NORMSDIST(-1)", 0.15865525393145707),
            ("NORMSDIST(0)", 0.5),
        ] {
            match value(formula, excel) {
                CellValue::Number(actual) => assert!((actual - expected).abs() < 1e-10),
                actual => panic!("{formula}: {actual:?}"),
            }
        }
        assert_eq!(
            value("PPMT(0.1,1,2,100,A6,A6)", excel),
            CellValue::Error(CellError::NA)
        );
        assert_eq!(
            value("NORMSDIST(0,TRUE)", excel),
            CellValue::Error(CellError::Value)
        );
    }
}
#[test]
fn operational_budgets_remain_visible_through_iferror() {
    for formula in [
        "IFERROR(IPMT(0.1,2,2,100,-16,1),7)",
        "IFERROR(PPMT(0,1,2,100),7)",
        "IFERROR(NORM.S.DIST(1,TRUE),7)",
    ] {
        assert!(
            matches!(
                run(
                    formula,
                    true,
                    EvaluationBudgets {
                        run_work: 1,
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
    let control = run("IFERROR(NORM.S.DIST(0,TRUE),7)", true, Default::default());
    assert!(matches!(
        &control,
        EvaluationOutcome::Complete {
            value: CellValue::Number(0.5),
            ..
        }
    ));
    let work = control.usage().work_units + 2;
    assert!(matches!(
        run(
            "IFERROR(NORM.S.DIST(1,TRUE),7)",
            true,
            EvaluationBudgets {
                run_work: work,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
    assert!(matches!(
        run(
            "IFERROR(IPMT(A1,1,A2,A3),7)",
            true,
            EvaluationBudgets {
                text_bytes: 1,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
}
