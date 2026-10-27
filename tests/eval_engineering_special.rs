//! Public special-function contracts from Microsoft and independently derived math.
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::eval::*;
use poirot::model::{CellError, CellValue, ExcelText, Workbook};
fn at(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}
fn book() -> Workbook {
    let mut b = Workbook::new();
    b.add_sheet("S").unwrap();
    b
}
fn value(result: EvaluationOutcome<CellValue>) -> CellValue {
    match result {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
fn call(b: &Workbook, formula: &str) -> CellValue {
    value(run_formula(
        b,
        0,
        at("Z100").address,
        formula,
        Default::default(),
        Default::default(),
    ))
}
fn near(value: CellValue, expected: f64) {
    let CellValue::Number(actual) = value else {
        panic!("{value:?}")
    };
    assert!(
        (actual - expected).abs() <= 1e-13 + expected.abs() * 2e-11,
        "{actual:e} != {expected:e}"
    );
}
#[test]
fn every_name_and_order_truncation_have_independent_answers() {
    let b = book();
    for (formula, expected) in [
        ("BESSELI(1,1)", 0.565159103992485),
        ("BESSELK(1,0)", 0.4210244382407083),
        ("BESSELY(1,0)", 0.08825696421567696),
        ("BESSELJ(1.9,2)", 0.3299257276923872),
        ("BESSELI(-1,1.9)", -0.565159103992485),
        ("BESSELJ(-1,1.9)", -0.4400505857449335),
        ("ERF(1)", 0.8427007929497149),
        ("ERF.PRECISE(-1)", -0.8427007929497149),
        ("ERFC(1)", 0.15729920705028513),
        ("ERFC.PRECISE(-1)", 1.8427007929497148),
        ("GESTEP(0)", 1.0),
        ("GESTEP(-1)", 0.0),
        ("GESTEP(2,2)", 1.0),
        ("ERF(1,0)", -0.8427007929497149),
    ] {
        near(call(&b, formula), expected);
    }
    assert_eq!(call(&b, "ISNUMBER(BESSELJ(10001,0))"), true.into());
    let CellValue::Number(tail) = call(&b, "ERFC(8)") else {
        panic!()
    };
    assert!((tail / 1.1224297172982926e-29 - 1.0).abs() < 2e-14);
    let CellValue::Number(interval) = call(&b, "ERF(6,8)") else {
        panic!()
    };
    assert!((interval / 2.151973671248769e-17 - 1.0).abs() < 2e-14);
}
#[test]
fn numeric_admission_optional_omission_errors_and_arity() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("A1").address, "1").unwrap();
    s.set_value(at("A2").address, true).unwrap();
    for formula in [
        "ERF(\"1\")",
        "ERF(A1)",
        "ERF(TRUE)",
        "ERF(A2)",
        "ERF(INDEX({1;2},1))",
    ] {
        near(call(&b, formula), 0.8427007929497149);
    }
    assert_eq!(call(&b, "ERF(A3)"), 0.0.into());
    assert_eq!(call(&b, "GESTEP(0,)"), 1.0.into());
    near(call(&b, "ERF(1,)"), -0.8427007929497149);
    for f in [
        "BESSELI(1,-0.1)",
        "BESSELJ(1,-0.1)",
        "BESSELK(0,0)",
        "BESSELY(-1,0)",
        "BESSELI(1000,0)",
    ] {
        assert_eq!(call(&b, f), CellValue::Error(CellError::Num), "{f}");
    }
    for f in [
        "ERF()",
        "ERF.PRECISE(1/0,1)",
        "ERFC(1/0,1)",
        "BESSELI(1/0)",
        "GESTEP(1/0,1,2)",
        "ERF(\"bad\")",
        "BESSELI(,1)",
    ] {
        assert_eq!(call(&b, f), CellValue::Error(CellError::Value), "{f}");
    }
    assert_eq!(call(&b, "ERF(1/0,#N/A)"), CellValue::Error(CellError::Div0));
    assert_eq!(call(&b, "IFERROR(BESSELK(-1,1),99)"), 99.0.into());
}
#[test]
fn broadcast_spills_overlay_edits_and_current_saved_state() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("A1").address, 0.).unwrap();
    s.set_value(at("A2").address, 1.).unwrap();
    s.set_formula(at("D1").address, "BESSELI(A1:A2,{0,1})")
        .unwrap();
    s.set_formula(at("G1").address, "GESTEP(A1:A2,1)").unwrap();
    s.set_formula(at("J1").address, "ERF.PRECISE(A1:A2)")
        .unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&b)
        .build(context.clone())
        .unwrap();
    near(direct.cell(at("E2")).unwrap(), 0.565159103992485);
    assert_eq!(direct.cell(at("D1")).unwrap(), 1.0.into());
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("E2"))
            .unwrap(),
        0.565159103992485,
    );
    overlay.set_value(at("A2"), 0.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("E2"))
            .unwrap(),
        0.0.into()
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("G2"))
            .unwrap(),
        0.0.into()
    );
    let (restored_book, state) = Overlay::from_snapshot(overlay.snapshot()).unwrap();
    let mut restored = state.attach(&restored_book).unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("J2"))
            .unwrap(),
        0.0.into()
    );
    restored.set_value(at("A2"), 1.).unwrap();
    near(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("J2"))
            .unwrap(),
        0.8427007929497149,
    );
}
#[test]
fn operational_limits_and_laziness_preserve_the_error_boundary() {
    let mut b = book();
    b.sheet_mut(0)
        .unwrap()
        .set_value(
            at("A1").address,
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800])),
        )
        .unwrap();
    for f in [
        "IFERROR(BESSELI(1,100001),99)",
        "IFERROR(BESSELI(100001,1),99)",
        "IFERROR(ERF(A1),99)",
    ] {
        assert!(
            matches!(
                run_formula(
                    &b,
                    0,
                    at("Z100").address,
                    f,
                    Default::default(),
                    Default::default()
                ),
                EvaluationOutcome::Aborted { .. }
            ),
            "{f}"
        );
    }
    for (f, budgets) in [
        (
            "IFERROR(BESSELJ(1,1000),99)",
            EvaluationBudgets {
                run_work: 100,
                ..Default::default()
            },
        ),
        (
            "IFERROR(BESSELK(1000,1000),99)",
            EvaluationBudgets {
                run_work: 100,
                ..Default::default()
            },
        ),
        (
            "IFERROR(BESSELI(500,500),99)",
            EvaluationBudgets {
                run_work: 100,
                ..Default::default()
            },
        ),
        (
            "IFERROR(ERF(SEQUENCE(100)),99)",
            EvaluationBudgets {
                materialized_values: 20,
                ..Default::default()
            },
        ),
    ] {
        assert!(
            matches!(
                run_formula(&b, 0, at("Z100").address, f, Default::default(), budgets),
                EvaluationOutcome::Aborted { .. }
            ),
            "{f}"
        );
    }
    assert_eq!(call(&b, "IF(FALSE,BESSELI(1,100001),7)"), 7.0.into());
}
#[test]
fn typed_engineering_kernels_broadcast_former_scalar_helpers() {
    // Excel's documented coefficient extraction and radix decoding apply to each
    // array member. DELTA compares numbers and BESSELJ(0,0) = 1 by its series.
    let mut workbook = book();
    for (address, formula) in [
        ("A1", "BIN2DEC({10;11})"),
        ("D1", "IMREAL({\"2+3i\";\"4+5j\"})"),
        ("G1", "COMPLEX({1;2},-1)"),
        ("J1", "DELTA({1;2},1)"),
        ("M1", "BESSELJ({0;0},{0,1})"),
    ] {
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula(at(address).address, formula)
            .unwrap();
    }
    let mut evaluation = Evaluation::with_workbook(&workbook)
        .build(EvaluationContext::default())
        .unwrap();
    for (address, expected) in [
        ("A1", CellValue::Number(2.0)),
        ("A2", CellValue::Number(3.0)),
        ("D1", CellValue::Number(2.0)),
        ("D2", CellValue::Number(4.0)),
        ("G1", CellValue::Text("1-i".into())),
        ("G2", CellValue::Text("2-i".into())),
        ("J1", CellValue::Number(1.0)),
        ("J2", CellValue::Number(0.0)),
        ("M2", CellValue::Number(1.0)),
        ("N2", CellValue::Number(0.0)),
    ] {
        assert_eq!(evaluation.cell(at(address)).unwrap(), expected, "{address}");
    }
}

#[test]
fn modern_negative_radix_values_validate_supplied_places() {
    // Microsoft documents nonnumeric places as VALUE even though negative
    // numbers ignore a successfully coerced numeric width.
    // https://support.microsoft.com/en-us/excel/functions/bin2hex-function
    // https://support.microsoft.com/en-au/excel/functions/dec2bin-function
    let workbook = book();
    for formula in ["DEC2BIN(-1,\"bad\")", "BIN2HEX(\"1111111111\",\"bad\")"] {
        assert_eq!(call(&workbook, formula), CellValue::Error(CellError::Value));
    }
    assert_eq!(
        call(&workbook, "DEC2BIN(-1,1)"),
        CellValue::Text("1111111111".into())
    );
    assert_eq!(
        call(&workbook, "BIN2HEX(\"1111111111\",1)"),
        CellValue::Text("FFFFFFFFFF".into())
    );
}

#[test]
fn lifted_engineering_helpers_publish_child_first_spills_in_both_evaluators() {
    let mut workbook = book();
    for (address, formula) in [
        ("A1", "BIN2DEC({10;11})"),
        ("D1", "COMPLEX({1;2},-1)"),
        ("G1", "DEC2HEX({15;16})"),
        ("J1", "DELTA({0;1})"),
    ] {
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula(at(address).address, formula)
            .unwrap();
    }
    let mut direct = Evaluation::with_workbook(&workbook)
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    let context = EvaluationContext::default();
    // Read spill children before their anchors, including both number and text.
    // DELTA defaults its omitted second number to zero:
    // https://support.microsoft.com/en-us/excel/functions/delta-function
    for (address, expected) in [
        ("A2", CellValue::Number(3.0)),
        ("D2", CellValue::Text("2-i".into())),
        ("G2", CellValue::Text("10".into())),
        ("J2", CellValue::Number(0.0)),
    ] {
        assert_eq!(direct.cell(at(address)).unwrap(), expected);
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at(address))
                .unwrap(),
            expected
        );
    }
}
