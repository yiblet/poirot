//! Independently calculated controls for typed mathematical scalar and collection roles.
//! Microsoft function definitions and spill behavior:
//! https://support.microsoft.com/en-us/excel/functions/math-and-trigonometry-functions-reference
//! https://support.microsoft.com/en-us/excel/formulas-and-functions/dynamic-array-formulas-and-spilled-array-behavior
use poirot::eval::{
    EvaluationTarget,
    overlay::{Evaluation, EvaluationContext, Overlay},
};
use poirot::model::{CellAddress, CellError, CellValue, Workbook};

/// Build a single-sheet arithmetic workbook.
fn book() -> Workbook {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    book
}
/// Address one cell in the arithmetic sheet.
fn target(row: u32, col: u32) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: CellAddress { row, col },
    }
}
/// Compare an independently calculated real result with floating-point tolerance.
fn number(value: CellValue, expected: f64) {
    let CellValue::Number(actual) = value else {
        panic!("expected {expected}, got {value:?}");
    };
    assert!(
        (actual - expected).abs() <= expected.abs().max(1.0) * 1e-12,
        "{actual} != {expected}"
    );
}

#[test]
fn scalar_numeric_kernels_spill_child_first_in_direct_and_overlay_evaluation() {
    let pi = std::f64::consts::PI;
    // Small integer, identity and elementary trigonometric controls are derived
    // algebraically from the documented functions, rather than cached Poirot output.
    let cases = [
        ("ABS({-1;-2})", 1., 2.),
        ("SIGN({-1;2})", -1., 1.),
        ("SQRT({1;4})", 1., 2.),
        ("EXP({0;1})", 1., std::f64::consts::E),
        ("LN({1;1})", 0., 0.),
        ("LOG10({10;100})", 1., 2.),
        ("LOG({8;16},2)", 3., 4.),
        ("POWER({2;3},2)", 4., 9.),
        ("MOD({5;8},3)", 2., 2.),
        ("INT({1.2;-1.2})", 1., -2.),
        ("TRUNC({1.2;-1.2})", 1., -1.),
        ("ROUND({1.25;2.25},1)", 1.3, 2.3),
        ("ROUNDUP({1.21;2.21},1)", 1.3, 2.3),
        ("ROUNDDOWN({1.29;2.29},1)", 1.2, 2.2),
        ("CEILING({1;3},2)", 2., 4.),
        ("FLOOR({1;3},2)", 0., 2.),
        ("SIN({0;0})", 0., 0.),
        ("COS({0;0})", 1., 1.),
        ("TAN({0;0})", 0., 0.),
        ("ASIN({0;1})", 0., pi / 2.),
        ("ACOS({1;0})", 0., pi / 2.),
        ("ATAN({0;1})", 0., pi / 4.),
        ("DEGREES({0;1})", 0., 180. / pi),
        ("RADIANS({0;180})", 0., pi),
        ("ACOSH({1;1})", 0., 0.),
        ("ASINH({0;0})", 0., 0.),
        ("ATANH({0;0})", 0., 0.),
        ("COSH({0;0})", 1., 1.),
        ("SINH({0;0})", 0., 0.),
        ("TANH({0;0})", 0., 0.),
        ("SQRTPI({0;1})", 0., pi.sqrt()),
        ("EVEN({1;3})", 2., 4.),
        ("ODD({2;4})", 3., 5.),
        ("FACT({3;4})", 6., 24.),
        ("FACTDOUBLE({5;6})", 15., 48.),
        ("ATAN2({1;0},{0;1})", 0., pi / 2.),
        ("QUOTIENT({7;9},2)", 3., 4.),
        ("COMBIN({4;5},2)", 6., 10.),
        ("COMBINA({3;4},2)", 6., 10.),
        ("MROUND({5;7},2)", 6., 8.),
        ("CEILING.MATH(-4.3,2,{0;1})", -4., -6.),
        ("FLOOR.MATH(-4.3,2,{0;1})", -6., -4.),
        ("CEILING.PRECISE({1;3},2)", 2., 4.),
        ("FLOOR.PRECISE({1;3},2)", 0., 2.),
        ("ACOT({1;-1})", pi / 4., 3. * pi / 4.),
        ("ACOTH({2;-2})", 3f64.ln() / 2., -3f64.ln() / 2.),
        ("COT({1;1})", 1. / 1f64.tan(), 1. / 1f64.tan()),
        ("COTH({1;1})", 1. / 1f64.tanh(), 1. / 1f64.tanh()),
        ("CSC({1;1})", 1. / 1f64.sin(), 1. / 1f64.sin()),
        ("CSCH({1;1})", 1. / 1f64.sinh(), 1. / 1f64.sinh()),
        ("SEC({0;0})", 1., 1.),
        ("SECH({0;0})", 1., 1.),
        ("ISO.CEILING({1;3},2)", 2., 4.),
        ("DECIMAL({10;11},2)", 2., 3.),
        ("SERIESSUM({1;2},0,1,{1;1;1})", 3., 7.),
        ("RANDBETWEEN({2;3},{2;3})", 2., 3.),
    ];
    let mut book = book();
    for (index, (formula, _, _)) in cases.iter().enumerate() {
        book.sheet_mut(0)
            .unwrap()
            .set_formula(target(0, (index * 2) as u32).address, *formula)
            .unwrap();
    }
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    for (index, (_, first, second)) in cases.iter().enumerate() {
        let col = (index * 2) as u32;
        // The unvisited child must discover its spill owner from result metadata.
        number(direct.cell(target(1, col)).unwrap(), *second);
        number(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target(1, col))
                .unwrap(),
            *second,
        );
        number(direct.cell(target(0, col)).unwrap(), *first);
        number(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target(0, col))
                .unwrap(),
            *first,
        );
    }
}

#[test]
fn text_spills_reductions_and_collection_shape_controls() {
    let mut book = book();
    for (col, formula) in [
        (0, "BASE({2;3},2)"),
        (2, "GCD({24;36},18)"),
        (4, "LCM({4;6},3)"),
        (6, "MULTINOMIAL({1;2},3)"),
        (8, "SERIESSUM(2,0,1,{1;1;1})"),
        (10, "GCD(9007199254740992,1)"),
        (12, "LCM(9007199254740992)"),
    ] {
        book.sheet_mut(0)
            .unwrap()
            .set_formula(target(0, col).address, formula)
            .unwrap();
    }
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    assert_eq!(
        direct.cell(target(1, 0)).unwrap(),
        CellValue::Text("11".into())
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target(1, 0))
            .unwrap(),
        CellValue::Text("11".into())
    );
    for (col, expected) in [(2, 6.), (4, 12.), (6, 60.), (8, 7.)] {
        assert_eq!(direct.cell(target(1, col)).unwrap(), CellValue::Blank);
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target(1, col))
                .unwrap(),
            CellValue::Blank
        );
        number(direct.cell(target(0, col)).unwrap(), expected);
        number(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target(0, col))
                .unwrap(),
            expected,
        );
    }
    for col in [10, 12] {
        assert_eq!(
            direct.cell(target(0, col)).unwrap(),
            CellValue::Error(CellError::Num)
        );
    }
}

#[test]
fn reference_spills_update_after_overlay_edits_and_keep_member_errors() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_value(target(0, 0).address, 3.)
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_value(target(1, 0).address, 4.)
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(target(0, 3).address, "FACT(A1:A2)")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(target(0, 5).address, "POWER({#N/A;2},{#REF!;3})")
        .unwrap();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    number(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target(1, 3))
            .unwrap(),
        24.,
    );
    overlay.set_value(target(1, 0), 5.).unwrap();
    number(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target(1, 3))
            .unwrap(),
        120.,
    );
    number(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target(1, 5))
            .unwrap(),
        8.,
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target(0, 5))
            .unwrap(),
        CellValue::Error(CellError::NA)
    );
}
