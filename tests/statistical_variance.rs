//! Sample and population moments follow their equations at large source offsets.
use poirot::{CellError, CellValue, Evaluation, EvaluationContext, Overlay, Workbook};

#[test]
fn variance_and_standard_deviation_share_stable_reduction_across_public_workflows() {
    // [0,0,1] has squared-deviation sum2/3, sample denominator2, population3.
    // The 2^40 translation below is exact for every binary64 operand.
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    for (row, delta) in [(0, 0.), (1, 0.), (2, 1.)] {
        sheet
            .set_value(
                poirot::CellAddress { row, col: 0 },
                1_099_511_627_776. + delta,
            )
            .unwrap();
    }
    sheet
        .set_value("A5".parse().unwrap(), "ignored reference text")
        .unwrap();
    sheet.set_value("A6".parse().unwrap(), true).unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let target = direct.target("Data!B1").unwrap();
    for (function, expected) in [
        ("VAR", 1. / 3.),
        ("VAR.S", 1. / 3.),
        ("VARP", 2. / 9.),
        ("VAR.P", 2. / 9.),
        ("STDEV", (1_f64 / 3.).sqrt()),
        ("STDEV.S", (1_f64 / 3.).sqrt()),
        ("STDEVP", (2_f64 / 9.).sqrt()),
        ("STDEV.P", (2_f64 / 9.).sqrt()),
    ] {
        let formula = format!("{function}(A1:A7)");
        direct.set_formula(target, &formula).unwrap();
        overlay.set_formula(target, &formula).unwrap();
        for actual in [
            direct.cell(target).unwrap(),
            overlay.evaluation(&context).unwrap().cell(target).unwrap(),
        ] {
            let CellValue::Number(actual) = actual else {
                panic!("{function}: {actual:?}")
            };
            assert!(
                (actual - expected).abs() <= 2e-15,
                "{function}: {actual} != {expected}"
            );
        }
    }
    for (formula, expected) in [
        ("STDEV(A1)", CellValue::Error(CellError::Div0)),
        ("VAR.S(A7)", CellValue::Error(CellError::Div0)),
        ("STDEV.P(A1)", CellValue::Number(0.)),
        ("VAR.P(A1:A4,#N/A)", CellValue::Error(CellError::NA)),
    ] {
        direct.set_formula(target, formula).unwrap();
        assert_eq!(direct.cell(target).unwrap(), expected, "{formula}");
    }
}
