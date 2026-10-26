//! Independent complex arithmetic identities and documented Microsoft contracts.
//! See each /excel/functions/im*-function page; expected coefficients derive
//! directly from Cartesian arithmetic and principal branch definitions.
use poirot::eval::{
    overlay::{Evaluation, EvaluationContext, Overlay},
    *,
};
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
fn near(book: &Workbook, formula: &str, expected: f64) {
    let CellValue::Number(actual) = call(book, formula) else {
        panic!("{formula}")
    };
    assert!(
        (actual - expected).abs() <= expected.abs().max(1.) * 2e-13,
        "{formula}: {actual} != {expected}"
    );
}
#[test]
fn numeric_and_text_channels_cover_every_new_operation() {
    let b = book();
    near(&b, "IMABS(\"3+4i\")", 5.);
    near(&b, "IMARGUMENT(\"-i\")", -std::f64::consts::FRAC_PI_2);
    assert_eq!(call(&b, "ISNUMBER(IMABS(\"3+4i\"))"), true.into());
    assert_eq!(call(&b, "ISTEXT(IMARGUMENT(\"i\"))"), false.into());
    for (formula, text) in [
        ("IMCONJUGATE(\"2-3j\")", "2+3j"),
        ("IMCOS(0)", "1"),
        ("IMCOSH(0)", "1"),
        ("IMEXP(0)", "1"),
        ("IMLN(1)", "0"),
        ("IMLOG10(1)", "0"),
        ("IMLOG2(1)", "0"),
        ("IMPOWER(\"i\",2)", "-1"),
        ("IMPRODUCT(\"1+i\",\"1-i\")", "2"),
        ("IMSEC(0)", "1"),
        ("IMSECH(0)", "1"),
        ("IMSIN(0)", "0"),
        ("IMSINH(0)", "0"),
        ("IMSQRT(\"3+4i\")", "2+i"),
        ("IMSUB(\"13+4i\",\"5+3i\")", "8+i"),
        ("IMSUM(\"1+2i\",\"3-5i\")", "4-3i"),
        ("IMDIV(\"3+4i\",\"1-2i\")", "-1+2i"),
        ("IMTAN(0)", "0"),
    ] {
        assert_eq!(call(&b, formula), CellValue::Text(text.into()), "{formula}");
        assert_eq!(
            call(&b, &format!("ISTEXT({formula})")),
            true.into(),
            "{formula}"
        );
    }
    for (name, real, imag) in [
        ("IMCOT", 0.00490118239430447, -0.999266927805902),
        ("IMCSC", -0.0754898329158637, 0.0648774713706355),
        ("IMCSCH", -0.036275889628626, -0.0051744731840194),
    ] {
        near(&b, &format!("IMREAL({name}(\"4+3i\"))"), real);
        near(&b, &format!("IMAGINARY({name}(\"4+3i\"))"), imag);
        assert_eq!(call(&b, &format!("ISTEXT({name}(\"4+3i\"))")), true.into());
    }
}
#[test]
fn suffixes_and_real_operands_are_preserved_without_roundtrips() {
    let b = book();
    for (formula, text) in [
        ("IMSUM(2,\"3j\")", "2+3j"),
        ("IMSUM(\"3j\",2)", "2+3j"),
        ("IMPRODUCT(2,\"3j\")", "6j"),
        ("IMDIV(2,\"j\")", "-2j"),
        ("IMSUB(2,\"j\")", "2-j"),
        ("IMPOWER(\"j\",-1)", "-j"),
        ("IMSUM(\"j\",\"-j\")", "0"),
        ("IMSUM(\"2+0j\",3)", "5"),
        ("IMCONJUGATE(\"1e-2-3e+2j\")", "0.01+300j"),
    ] {
        assert_eq!(call(&b, formula), CellValue::Text(text.into()), "{formula}");
    }
    for formula in [
        "IMSUM(\"i\",\"j\")",
        "IMPRODUCT(\"i\",\"j\")",
        "IMDIV(\"i\",\"j\")",
        "IMSUB(\"2+0i\",\"3+0j\")",
    ] {
        assert_eq!(
            call(&b, formula),
            CellValue::Error(CellError::Num),
            "{formula}"
        );
    }
}
#[test]
fn rectangular_lifting_broadcasts_and_keeps_errors_per_member() {
    let b = book();
    assert_eq!(
        call(&b, "INDEX(IMSUB({\"1+i\";\"2+i\"},{1,2}),2,2)"),
        "i".into()
    );
    near(&b, "ROWS(IMABS({\"3+4i\";\"5+12i\"}))", 2.);
    near(&b, "INDEX(IMABS({\"3+4i\";\"5+12i\"}),2,1)", 13.);
    assert_eq!(
        call(&b, "INDEX(IMCONJUGATE({\"i\";TRUE;\"j\"}),2,1)"),
        CellValue::Error(CellError::Value)
    );
    assert_eq!(
        call(&b, "INDEX(IMDIV({\"i\";\"j\"},{1;2;3}),3,1)"),
        CellValue::Error(CellError::NA)
    );
    assert_eq!(
        call(&b, "IMSUM({\"i\",\"2i\";\"3i\",\"4i\"})"),
        "10i".into()
    );
    assert_eq!(call(&b, "IMPRODUCT({\"i\",\"-i\";2,3})"), "6".into());
    assert_eq!(call(&b, "IMCONJUGATE({\"2+j\"})"), "2-j".into());
    assert_eq!(
        call(&b, "INDEX(IMPOWER({\"i\";\"2\"},{2,-1}),2,2)"),
        "0.5".into()
    );
}
#[test]
fn reductions_consume_ranges_unions_and_errors_in_source_order() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    for (cell, text) in [("A1", "i"), ("A2", "2i"), ("B1", "3i"), ("B2", "4i")] {
        s.set_value(at(cell).address, text).unwrap();
    }
    s.set_value(at("C1").address, true).unwrap();
    s.set_value(at("C2").address, CellValue::Error(CellError::NA))
        .unwrap();
    assert_eq!(call(&b, "IMSUM(A1:B2)"), "10i".into());
    assert_eq!(call(&b, "IMSUM((A1:A2,B1:B2))"), "10i".into());
    assert_eq!(call(&b, "IMPRODUCT(A1:A2)"), "-2".into());
    assert_eq!(
        call(&b, "IMSUM(A1:B2,C1)"),
        CellValue::Error(CellError::Value)
    );
    assert_eq!(call(&b, "IMSUM(C2,C1)"), CellValue::Error(CellError::NA));
    assert_eq!(call(&b, "IMSUM(C1,C2)"), CellValue::Error(CellError::Value));
    assert_eq!(call(&b, "IMSUM(D1:D2)"), "0".into());
    assert_eq!(call(&b, "IMPRODUCT(3,D1)"), "0".into());
    assert_eq!(call(&b, "IMSUM(\"\")"), CellValue::Error(CellError::Num));
    assert_eq!(call(&b, "IMSUM(\"i\")"), "i".into());
    assert_eq!(call(&b, "IMPRODUCT(\"i\")"), "i".into());
}
#[test]
fn principal_domains_near_poles_and_large_finite_inputs() {
    let b = book();
    for name in ["IMLN", "IMLOG10", "IMLOG2", "IMCOT", "IMCSC", "IMCSCH"] {
        assert_eq!(
            call(&b, &format!("{name}(0)")),
            CellValue::Error(CellError::Num)
        );
    }
    assert_eq!(call(&b, "IMARGUMENT(0)"), CellValue::Error(CellError::Div0));
    assert_eq!(call(&b, "IMDIV(1,0)"), CellValue::Error(CellError::Num));
    assert_eq!(call(&b, "IMPOWER(0,0)"), CellValue::Error(CellError::Num));
    assert_eq!(call(&b, "IMPOWER(0,-1)"), CellValue::Error(CellError::Num));
    assert_eq!(call(&b, "IMPOWER(0,0.5)"), "0".into());
    assert_eq!(call(&b, "IMPOWER(4,0.5)"), "2".into());
    assert_eq!(call(&b, "IMSQRT(\"-4-0j\")"), "2j".into());
    near(&b, "IMARGUMENT(\"-4-0j\")", std::f64::consts::PI);
    assert_eq!(call(&b, "IMTAN(\"2+1000i\")"), "i".into());
    assert_eq!(call(&b, "IMCOT(\"2-1000i\")"), "i".into());
    assert_eq!(
        call(&b, "IMDIV(\"1e308+1e308i\",\"1e308+1e308i\")"),
        "1".into()
    );
    near(&b, "IMABS(\"3e200+4e200i\")", 5e200);
    near(&b, "IMREAL(IMLN(\"1e308+1e308i\"))", 709.542782232446);
    near(&b, "IMREAL(IMCSC(\"1e-200\"))", 1e200);
    assert_eq!(call(&b, "IMEXP(1000)"), CellValue::Error(CellError::Num));
    assert_eq!(
        call(&b, "IMPOWER(2,\"bad\")"),
        CellValue::Error(CellError::Value)
    );
}
#[test]
fn arity_laziness_and_operational_limits_do_not_execute_unneeded_operands() {
    let b = book();
    for name in ["IMSUM", "IMPRODUCT"] {
        let valid = format!("{name}({})", vec!["1"; 255].join(","));
        let expected = if name == "IMSUM" { "255" } else { "1" };
        assert_eq!(call(&b, &valid), expected.into());
        for formula in [
            format!("{name}()"),
            format!("{name}({})", vec!["RAND()"; 256].join(",")),
        ] {
            let result = run_formula(
                &b,
                0,
                at("Z100").address,
                &formula,
                Default::default(),
                Default::default(),
            );
            assert!(result.usage().random_accesses.is_empty());
            assert_eq!(value(result), CellValue::Error(CellError::Value));
        }
    }
    let result = run_formula(
        &b,
        0,
        at("Z100").address,
        "IMDIV(RAND())",
        Default::default(),
        Default::default(),
    );
    assert!(result.usage().random_accesses.is_empty());
    assert_eq!(value(result), CellValue::Error(CellError::Value));
    near(&b, "IF(FALSE,IMSUM(RAND()),7)", 7.);
    for (formula, budgets) in [
        (
            "IFERROR(IMSUM(A1:A1000),99)",
            EvaluationBudgets {
                run_work: 100,
                ..Default::default()
            },
        ),
        (
            "IFERROR(IMCONJUGATE(\"2+i\"),99)",
            EvaluationBudgets {
                text_bytes: 10,
                ..Default::default()
            },
        ),
        (
            "IFERROR(IMABS(SEQUENCE(100)),99)",
            EvaluationBudgets {
                materialized_values: 20,
                ..Default::default()
            },
        ),
    ] {
        let result = run_formula(
            &b,
            0,
            at("Z100").address,
            formula,
            Default::default(),
            budgets,
        );
        assert!(
            matches!(result, EvaluationOutcome::Aborted { .. }),
            "{formula}"
        );
    }
    let result = run_formula(
        &b,
        0,
        at("Z100").address,
        "IFERROR(IMSUM(,),99)",
        Default::default(),
        Default::default(),
    );
    assert!(matches!(result, EvaluationOutcome::Aborted { .. }));
    near(&b, "IFERROR(IMCONJUGATE(,),99)", 99.);
}
#[test]
fn direct_overlay_edits_spill_children_and_saved_state_remain_coherent() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("A1").address, "3+4j").unwrap();
    s.set_value(at("A2").address, "5+12j").unwrap();
    s.set_formula(at("D1").address, "IMCONJUGATE(A1:A2)")
        .unwrap();
    s.set_formula(at("G1").address, "IMSUM(A1:A3)").unwrap();
    s.set_formula(at("J1").address, "IMABS(A1:A2)").unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&b)
        .build(context.clone())
        .unwrap();
    assert_eq!(direct.cell(at("D2")).unwrap(), "5-12j".into());
    assert_eq!(direct.cell(at("J2")).unwrap(), 13.0.into());
    assert_eq!(direct.cell(at("G1")).unwrap(), "8+16j".into());
    direct.set_value(at("A2"), "2j").unwrap();
    assert_eq!(direct.cell(at("D2")).unwrap(), "-2j".into());
    assert_eq!(direct.cell(at("G1")).unwrap(), "3+6j".into());
    let mut overlay = Overlay::with_workbook(b).build().unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        "5-12j".into()
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("G1"))
            .unwrap(),
        "8+16j".into()
    );
    overlay.set_value(at("A3"), "j").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("G1"))
            .unwrap(),
        "8+17j".into()
    );
    overlay.set_value(at("D2"), 9.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D1"))
            .unwrap(),
        CellValue::Error(CellError::Spill)
    );
    overlay.set_value(at("D2"), CellValue::Blank).unwrap();
    overlay.set_formula(at("D1"), "IMCONJUGATE(A1)").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        CellValue::Blank
    );
    let snapshot = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (book, state) = Overlay::from_snapshot(postcard::from_bytes(&snapshot).unwrap()).unwrap();
    let mut restored = state.attach(&book).unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("G1"))
            .unwrap(),
        "8+17j".into()
    );
}
#[test]
fn profile_alias_and_checked_utf16_admission_preserve_existing_contracts() {
    let mut b = book();
    b.sheet_mut(0)
        .unwrap()
        .set_value(
            at("A1").address,
            CellValue::TextUtf16(ExcelText::from_units(
                "2+j".encode_utf16().collect::<Vec<_>>(),
            )),
        )
        .unwrap();
    b.sheet_mut(0)
        .unwrap()
        .set_value(
            at("A2").address,
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800])),
        )
        .unwrap();
    assert_eq!(call(&b, "IMCONJUGATE(A1)"), "2-j".into());
    let result = run_formula(
        &b,
        0,
        at("Z100").address,
        "IFERROR(IMCONJUGATE(A2),7)",
        Default::default(),
        Default::default(),
    );
    assert!(matches!(result, EvaluationOutcome::Aborted { .. }));
    for formula in [
        "IMABS(\"3+4i\")",
        "IMCONJUGATE(\"j\")",
        "IMSUM(\"i\")",
        "IMPOWER(\"j\",2)",
    ] {
        assert_eq!(call(&b, &format!("_xlfn.{formula}")), call(&b, formula));
        let pinned = run_formula_with_configuration(
            &b,
            0,
            at("Z100").address,
            formula,
            &RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi),
            Default::default(),
        );
        assert_eq!(value(pinned), CellValue::Error(CellError::Name));
    }
    for formula in ["IMABS(TRUE)", "IMCONJUGATE(FALSE)", "IMSUM({TRUE})"] {
        assert_eq!(call(&b, formula), CellValue::Error(CellError::Value));
    }
    for formula in [
        "IMCONJUGATE(\"\")",
        "IMCONJUGATE(\"1+2I\")",
        "IMCONJUGATE(\"1e309i\")",
    ] {
        assert_eq!(call(&b, formula), CellValue::Error(CellError::Num));
    }
    assert_eq!(
        call(&b, "IMCONJUGATE(#REF!)"),
        CellValue::Error(CellError::Ref)
    );
}
