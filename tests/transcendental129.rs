//! Exact retained native observations exercised through actual evaluator dispatch.
use poirot::eval::{self as eval, EvaluationInputs, EvaluationOutcome};
use poirot::model::{CellError, CellValue, Workbook};
fn value(formula: &str) -> CellValue {
    let mut w = Workbook::new();
    w.add_sheet("S").unwrap();
    match eval::run_formula(
        &w,
        0,
        "A1".parse().unwrap(),
        formula,
        EvaluationInputs::default(),
        Default::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{formula}: {other:?}"),
    }
}
#[test]
fn retained_exp_and_log10_native_bits_pass_actual_dispatch() {
    for (formula, bits) in [
        ("EXP(2)", 0x401d8e64b8d4ddadu64),
        ("LOG10(2)", 0x3fd34413509f79feu64),
    ] {
        let CellValue::Number(actual) = value(formula) else {
            panic!("{formula}")
        };
        assert_eq!(actual.to_bits(), bits, "{formula}");
    }
}
#[test]
fn actual_dispatch_preserves_zero_and_domain_boundaries() {
    for formula in ["EXP(0)", "EXP(-0)"] {
        assert_eq!(value(formula), CellValue::Number(1.));
    }
    let CellValue::Number(actual) = value("LOG10(1)") else {
        panic!("LOG10")
    };
    assert_eq!(actual.to_bits(), 0);
    for formula in ["LOG10(0)", "LOG10(-0)", "LOG10(-1)", "EXP(1000)"] {
        assert_eq!(value(formula), CellValue::Error(CellError::Num));
    }
}
