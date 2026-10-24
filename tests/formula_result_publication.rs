//! Excel worksheet publication consumes a blank; nested expressions retain it.
use poirot::{
    CellValue, EvaluationContext, EvaluationTarget, Workbook, eval::overlay::Evaluation,
    eval::overlay::Overlay,
};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
fn source() -> Workbook {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet.set_value(at("J1").address, 1.).unwrap();
    workbook
}

macro_rules! publication_test {
    ($name:ident, $build:expr, $read:expr) => {
        #[test]
        fn $name() {
            let source = source();
            let mut runtime = ($build)(&source);
            let read = $read;
            // User-confirmed Excel numeric publication, challenged across distinct selectors.
            for selected in [
                "A1",
                "IF(TRUE,A1,1/0)",
                "INDEX(A1:A2,1)",
                "CHOOSE(1,A1,1/0)",
                "IFERROR(A1,42)",
                "IFNA(A1,42)",
                "VLOOKUP(1,J1:K1,2,FALSE)",
                "IFERROR(INDEX(A1:A2,1),42)",
            ] {
                runtime.set_formula(at("B1"), selected).unwrap();
                runtime
                    .set_formula(at("C1"), format!("ISBLANK({selected})"))
                    .unwrap();
                runtime
                    .set_formula(at("C2"), format!("ISNUMBER({selected})"))
                    .unwrap();
                runtime.set_formula(at("D1"), "ISBLANK(B1)").unwrap();
                runtime.set_formula(at("D2"), "ISNUMBER(B1)").unwrap();
                runtime.set_formula(at("D3"), "B1").unwrap();
                for _ in 0..2 {
                    // Reach B1 recursively first, then challenge retained memoization.
                    assert_eq!(
                        read(&mut runtime, at("D2")),
                        CellValue::Boolean(true),
                        "{selected}"
                    );
                    assert_eq!(
                        read(&mut runtime, at("B1")),
                        CellValue::Number(0.),
                        "{selected}"
                    );
                    assert_eq!(
                        read(&mut runtime, at("C1")),
                        CellValue::Boolean(true),
                        "{selected}"
                    );
                    assert_eq!(
                        read(&mut runtime, at("C2")),
                        CellValue::Boolean(false),
                        "{selected}"
                    );
                    assert_eq!(
                        read(&mut runtime, at("D1")),
                        CellValue::Boolean(false),
                        "{selected}"
                    );
                    assert_eq!(
                        read(&mut runtime, at("D3")),
                        CellValue::Number(0.),
                        "{selected}"
                    );
                    assert_eq!(read(&mut runtime, at("A1")), CellValue::Blank);
                }
            }
            runtime.set_formula(at("B1"), "A1").unwrap();
            runtime.set_formula(at("B2"), "2").unwrap();
            for (formula, expected) in [
                ("COUNT(B:B)", 2.),
                ("COUNT(A:A)", 0.),
                ("AVERAGE(B:B)", 1.),
                ("MAX(B1,-1)", 0.),
            ] {
                runtime.set_formula(at("Z100"), formula).unwrap();
                assert_eq!(
                    read(&mut runtime, at("Z100")),
                    CellValue::Number(expected),
                    "{formula}"
                );
            }
            // A physical blank is ignored, while the published formula zero is textified.
            for (formula, expected) in [
                (r#"TEXTJOIN("-",TRUE,A1,"C")"#, "C"),
                (r#"TEXTJOIN("-",TRUE,B1,"C")"#, "0-C"),
            ] {
                runtime.set_formula(at("Z100"), formula).unwrap();
                assert_eq!(
                    read(&mut runtime, at("Z100")),
                    CellValue::Text(expected.into()),
                    "{formula}"
                );
            }
            // Edits must invalidate the published zero, preserving empty text and booleans.
            for value in [
                CellValue::Text(String::new()),
                CellValue::Number(7.),
                CellValue::Boolean(false),
                CellValue::Blank,
            ] {
                runtime.set_value(at("A1"), value.clone()).unwrap();
                let expected = if value == CellValue::Blank {
                    CellValue::Number(0.)
                } else {
                    value
                };
                assert_eq!(read(&mut runtime, at("D3")), expected);
            }
            runtime.set_formula(at("B1"), "\"\"").unwrap();
            assert_eq!(read(&mut runtime, at("B1")), CellValue::Text(String::new()));
            runtime
                .set_formula(at("Z100"), "ISBLANK(IF(TRUE,,))")
                .unwrap();
            assert_eq!(read(&mut runtime, at("Z100")), CellValue::Boolean(true));
            runtime.set_formula(at("Z100"), "IF(TRUE,,)").unwrap();
            assert_eq!(read(&mut runtime, at("Z100")), CellValue::Number(0.));
        }
    };
}
publication_test!(
    direct_formula_results_publish_after_expression_evaluation,
    |source| Evaluation::with_workbook(source)
        .build(EvaluationContext::default())
        .unwrap(),
    |runtime: &mut Evaluation<'_, &'_ Workbook>, target| runtime.cell(target).unwrap()
);
publication_test!(
    overlay_formula_results_publish_after_expression_evaluation,
    |source| Overlay::with_workbook(source).build().unwrap(),
    |runtime: &mut Overlay<'_, &'_ Workbook>, target| runtime
        .evaluation(&EvaluationContext::default())
        .unwrap()
        .cell(target)
        .unwrap()
);

#[test]
fn worksheet_zero_survives_retained_snapshot_but_an_absent_input_stays_blank() {
    let source = source();
    let context = EvaluationContext::default();
    let mut runtime = Overlay::with_workbook(&source).build().unwrap();
    runtime.set_formula(at("B1"), "A1").unwrap();
    runtime.set_formula(at("C1"), "ISNUMBER(B1)").unwrap();
    assert_eq!(
        runtime
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Boolean(true)
    );
    let bytes = postcard::to_allocvec(&runtime.snapshot()).unwrap();
    let (restored, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut runtime = state.attach(&restored).unwrap();
    let mut evaluation = runtime.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("B1")).unwrap(), CellValue::Number(0.));
    assert_eq!(evaluation.cell(at("C1")).unwrap(), CellValue::Boolean(true));
    assert_eq!(evaluation.usage().formula_executions, 0);
    assert_eq!(evaluation.cell(at("A1")).unwrap(), CellValue::Blank);
}
