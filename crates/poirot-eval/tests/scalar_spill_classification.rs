//! Scalar discovery skips source binding; reached execution must bind its own coordinates.
//! SUM expectations use addition of the addressed numeric values, following
//! https://support.microsoft.com/en-us/excel/functions/sum-function .
//! Preparation counts and lazy compilation are Poirot contracts.
use poirot_eval::{
    EvaluationTarget,
    overlay::{Evaluation, EvaluationContext, Overlay},
};
use poirot_model::{CellValue, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

#[test]
fn scalar_discovery_reuses_proofs_but_evaluates_the_reached_source() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for row in 1..=128 {
        sheet
            .set_value(at(&format!("A{row}")).address, f64::from(row))
            .unwrap();
        sheet
            .set_formula(
                at(&format!("B{row}")).address,
                format!("SUM(A{row}:A{row})"),
            )
            .unwrap();
    }
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    assert_eq!(direct.cell(at("C1")).unwrap(), CellValue::Blank);
    assert_eq!(direct.compiled_usage().parse_attempts, 1);
    assert_eq!(direct.cell(at("B128")).unwrap(), CellValue::Number(128.));
    direct.set_value(at("A128"), 7.).unwrap();
    assert_eq!(direct.cell(at("B128")).unwrap(), CellValue::Number(7.));

    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    {
        let mut evaluation = overlay.evaluation(&context).unwrap();
        assert_eq!(evaluation.cell(at("C1")).unwrap(), CellValue::Blank);
        assert_eq!(
            evaluation.cell(at("B128")).unwrap(),
            CellValue::Number(128.)
        );
        evaluation.finish();
    }
    overlay.set_value(at("A128"), 7.).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("B128")).unwrap(), CellValue::Number(7.));
}
