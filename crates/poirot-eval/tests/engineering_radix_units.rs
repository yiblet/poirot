//! Public engineering contracts from Microsoft syntax and positional arithmetic.
use poirot_eval::{
    overlay::{Evaluation, EvaluationContext, Overlay},
    *,
};
use poirot_model::{CellError, CellValue, ExcelText, Workbook};
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
fn call(book: &Workbook, formula: &str) -> CellValue {
    value(run_formula(
        book,
        0,
        at("Z100").address,
        formula,
        Default::default(),
        Default::default(),
    ))
}
#[test]
fn all_thirteen_functions_have_independent_success_cases() {
    let b = book();
    for (formula, text) in [
        ("BIN2HEX(\"1111111111\")", "FFFFFFFFFF"),
        ("BIN2OCT(1001,3)", "011"),
        ("DEC2OCT(58,3)", "072"),
        ("HEX2BIN(\"1FF\")", "111111111"),
        ("HEX2OCT(\"FF\")", "377"),
        ("OCT2BIN(\"7777777777\")", "1111111111"),
        ("OCT2HEX(\"7777777777\")", "FFFFFFFFFF"),
    ] {
        assert_eq!(call(&b, formula), text.into(), "{formula}");
    }
    for (formula, n) in [
        ("BITAND(10,12)", 8.),
        ("BITOR(10,12)", 14.),
        ("BITXOR(10,12)", 6.),
        ("BITLSHIFT(3,2)", 12.),
        ("BITRSHIFT(13,2)", 3.),
        ("CONVERT(1,\"byte\",\"bit\")", 8.),
    ] {
        assert_eq!(call(&b, formula), n.into(), "{formula}");
    }
    assert_eq!(call(&b, "ISTEXT(DEC2OCT(8))"), true.into());
    assert_eq!(call(&b, "ISNUMBER(BITAND(3,1))"), true.into());
}
#[test]
fn signed_widths_strict_grammar_places_and_error_order() {
    let b = book();
    for f in [
        "HEX2BIN(\"200\")",
        "HEX2BIN(\"FFFFFFFDFF\")",
        "OCT2BIN(1000)",
        "BIN2HEX(2)",
        "BIN2OCT(\"１２\")",
        "DEC2OCT(536870912)",
        "DEC2OCT(536870911.5)",
        "DEC2OCT(-536870912.5)",
        "DEC2OCT(-536870913)",
        "HEX2OCT(\"20000000\")",
        "DEC2OCT(8,1)",
        "DEC2OCT(1,0)",
        "DEC2OCT(1,11)",
    ] {
        assert_eq!(call(&b, f), CellValue::Error(CellError::Num), "{f}");
    }
    assert_eq!(call(&b, "HEX2BIN(\"FFFFFFFE00\")"), "1000000000".into());
    assert_eq!(call(&b, "DEC2OCT(8,4.9)"), "0010".into());
    assert_eq!(
        call(&b, "DEC2OCT(8,\"bad\")"),
        CellValue::Error(CellError::Value)
    );
    assert_eq!(call(&b, "DEC2OCT(8,)"), "10".into());
    assert_eq!(
        call(&b, "BIN2HEX(\"bad\",1/0)"),
        CellValue::Error(CellError::Num)
    );
    // Valid padding widths do not change a negative two's-complement result;
    // errors in supplied operands still propagate through normal argument preparation.
    assert_eq!(call(&b, "DEC2OCT(-1,1)"), "7777777777".into());
    assert_eq!(
        call(&b, "DEC2OCT(-1,1/0)"),
        CellValue::Error(CellError::Div0)
    );
    assert_eq!(call(&b, "DEC2OCT(,3)"), CellValue::Error(CellError::Value));
}
#[test]
fn bit_domains_origins_and_unit_validation() {
    let mut b = book();
    b.sheet_mut(0)
        .unwrap()
        .set_value(at("A1").address, "10")
        .unwrap();
    for f in [
        "BITAND(-1,0)",
        "BITOR(0.5,0)",
        "BITXOR(281474976710656,0)",
        "BITLSHIFT(1,54)",
        "BITRSHIFT(1,0.5)",
        "BITLSHIFT(140737488355328,1)",
    ] {
        assert_eq!(call(&b, f), CellValue::Error(CellError::Num), "{f}");
    }
    assert_eq!(call(&b, "BITAND(A1,12)"), 8.0.into());
    let CellValue::Number(tiny) = call(&b, "CONVERT(1e-20,\"K\",\"mK\")") else {
        panic!()
    };
    assert!((tiny / 1e-17 - 1.0).abs() < 1e-14);
    assert_eq!(call(&b, "CONVERT(32,\"F\",\"C\")"), 0.0.into());
    assert_eq!(call(&b, "BITOR(TRUE,2)"), 3.0.into());
    assert_eq!(call(&b, "BITAND(B1,2)"), 0.0.into());
    for f in [
        "CONVERT(1,\"x\",\"x\")",
        "CONVERT(1,\"m\",\"s\")",
        "CONVERT(1,\"kim\",\"m\")",
        "CONVERT(1,\"KiB\",\"byte\")",
    ] {
        assert_eq!(call(&b, f), CellValue::Error(CellError::NA), "{f}");
    }
    for formula in [
        "CONVERT(1,1,\"m\")",
        "CONVERT(1,\"m\",TRUE)",
        "CONVERT(1,,\"m\")",
    ] {
        assert_eq!(call(&b, formula), CellValue::Error(CellError::Value));
    }
    assert_eq!(
        call(&b, "CONVERT(1,#DIV/0!,\"m\")"),
        CellValue::Error(CellError::Div0)
    );
    assert_eq!(
        call(&b, "BITAND(\"bad\",1)"),
        CellValue::Error(CellError::Value)
    );
    assert_eq!(
        call(&b, "CONVERT(\"bad\",\"m\",\"m\")"),
        CellValue::Error(CellError::Value)
    );
    let CellValue::Number(n) = call(&b, "CONVERT(68,\"F\",\"C\")") else {
        panic!()
    };
    assert!((n - 20.).abs() < 1e-12);
    let CellValue::Number(n) = call(&b, "CONVERT(1,\"cm3\",\"m3\")") else {
        panic!()
    };
    assert!((n - 1e-6).abs() < 1e-20);
}
#[test]
fn direct_overlay_spills_edits_and_saved_state() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("A1").address, 8.).unwrap();
    s.set_value(at("A2").address, 64.).unwrap();
    s.set_formula(at("D1").address, "DEC2OCT(A1:A2)").unwrap();
    s.set_formula(at("G1").address, "CONVERT(A1:A2,\"byte\",\"bit\")")
        .unwrap();
    s.set_formula(at("J1").address, "BITOR(A1:A2,1)").unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&b)
        .build(context.clone())
        .unwrap();
    assert_eq!(direct.cell(at("D2")).unwrap(), "100".into());
    assert_eq!(direct.cell(at("G2")).unwrap(), 512.0.into());
    assert_eq!(direct.cell(at("J2")).unwrap(), 65.0.into());
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        "100".into()
    );
    overlay.set_value(at("A2"), 16.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        "20".into()
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("G2"))
            .unwrap(),
        128.0.into()
    );
    overlay.set_formula(at("D1"), "DEC2OCT(A1)").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        CellValue::Blank
    );
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (book, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach(&book).unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("G2"))
            .unwrap(),
        128.0.into()
    );
}
#[test]
fn resource_failures_are_not_worksheet_values_and_utf16_is_checked() {
    let mut b = book();
    b.sheet_mut(0)
        .unwrap()
        .set_value(
            at("A1").address,
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800])),
        )
        .unwrap();
    let result = run_formula(
        &b,
        0,
        at("Z100").address,
        "IFERROR(BIN2HEX(A1),99)",
        Default::default(),
        Default::default(),
    );
    assert!(matches!(result, EvaluationOutcome::Aborted { .. }));
    for (formula, budgets) in [
        (
            "IFERROR(DEC2OCT(SEQUENCE(100)),99)",
            EvaluationBudgets {
                materialized_values: 20,
                ..Default::default()
            },
        ),
        (
            "IFERROR(CONVERT(1,\"meters\",\"m\"),99)",
            EvaluationBudgets {
                text_bytes: 2,
                ..Default::default()
            },
        ),
        (
            "IFERROR(BITAND(SEQUENCE(100),3),99)",
            EvaluationBudgets {
                run_work: 10,
                ..Default::default()
            },
        ),
    ] {
        assert!(
            matches!(
                run_formula(
                    &b,
                    0,
                    at("Z100").address,
                    formula,
                    Default::default(),
                    budgets
                ),
                EvaluationOutcome::Aborted { .. }
            ),
            "{formula}"
        );
    }
}

#[test]
fn modern_registration_preserves_pinned_profile_and_lazy_dependencies() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("A1").address, -1.).unwrap();
    s.set_formula(at("D1").address, "DEC2OCT(A1,RAND())")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    let context = EvaluationContext::default();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at("D1")).unwrap(), "7777777777".into());
    }
    overlay.set_value(at("A1"), 8.).unwrap();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(
            run.cell(at("D1")).unwrap(),
            CellValue::Error(CellError::Num)
        );
    }
    for formula in ["BIN2HEX(1)", "BITAND(1,1)", "CONVERT(1,\"m\",\"m\")"] {
        let result = run_formula_with_configuration(
            &b,
            0,
            at("Z100").address,
            formula,
            &RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi),
            Default::default(),
        );
        assert_eq!(value(result), CellValue::Error(CellError::Name));
    }
}
