//! Public special-function contracts from Microsoft and independently derived math.
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
fn pinned_besselj_execution_and_new_function_availability_are_preserved() {
    let b = book();
    let configuration =
        RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi);
    for f in ["BESSELI(1,1)", "ERF(1)", "ERFC.PRECISE(1)", "GESTEP(1)"] {
        assert_eq!(
            value(run_formula_with_configuration(
                &b,
                0,
                at("Z100").address,
                f,
                &configuration,
                Default::default()
            )),
            CellValue::Error(CellError::Name)
        );
    }
    for f in ["BESSELJ(-1,1)", "BESSELJ(10001,0)"] {
        assert_eq!(
            value(run_formula_with_configuration(
                &b,
                0,
                at("Z100").address,
                f,
                &configuration,
                Default::default()
            )),
            CellValue::Error(CellError::Num)
        );
    }
    near(
        value(run_formula_with_configuration(
            &b,
            0,
            at("Z100").address,
            "BESSELJ(1.9,2)",
            &configuration,
            Default::default(),
        )),
        0.3299257276923872,
    );
}
#[test]
fn pinned_scalar_besselj_discovery_is_explicit_under_dynamic_publication() {
    let mut b = book();
    b.sheet_mut(0)
        .unwrap()
        .set_formula(at("D1").address, "BESSELJ({1;2},RAND())")
        .unwrap();
    let policy = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&b, &policy);
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        let configuration = RunConfiguration::default()
            .with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi)
            .with_array_result_policy(arrays);
        let mut run = EvaluationRun::new_with_configuration(
            WorkbookView::new(&b, &plan, &policy).unwrap(),
            &configuration,
            Default::default(),
        )
        .unwrap();
        assert_eq!(run.evaluate_cell(at("D2")).unwrap(), CellValue::Blank);
        let finished = run.finish_retaining_cells(Ok(()));
        assert_eq!(finished.report.execution.formula_executions, 0);
        let EvaluationOutcome::Complete { usage, .. } = finished.report.outcome else {
            panic!()
        };
        assert!(usage.random_accesses.is_empty());
    }
}
