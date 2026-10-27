use poirot::eval::{
    BuiltinSemanticsPolicy, EvaluationBudgets, EvaluationOutcome, RunConfiguration,
    run_formula_with_configuration,
};
use poirot::model::{CellAddress, CellError, CellValue, Workbook};

fn workbook() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    for (row, n) in [1., 4., 4., 9.].into_iter().enumerate() {
        w.sheet_mut(0)
            .unwrap()
            .set_value(
                CellAddress {
                    row: row as u32,
                    col: 0,
                },
                n,
            )
            .unwrap();
    }
    w
}
fn excel() -> RunConfiguration {
    RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation)
}
fn run(
    source: &str,
    configuration: &RunConfiguration,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        &workbook(),
        0,
        CellAddress { row: 0, col: 25 },
        source,
        configuration,
        budgets,
    )
}
fn value(source: &str, configuration: &RunConfiguration) -> CellValue {
    match run(source, configuration, EvaluationBudgets::default()) {
        EvaluationOutcome::Complete { value, .. } => value,
        result => panic!("{source}: {result:?}"),
    }
}
#[test]
fn documented_aliases_are_default_and_pinned_registry_remains_explicit() {
    let default =
        RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi);
    let excel = excel();
    assert_eq!(default.builtin_policy(), BuiltinSemanticsPolicy::PinnedPoi);
    assert_ne!(
        default.fingerprint().computation(),
        excel.fingerprint().computation()
    );
    assert_eq!(
        poirot::eval::functions::registry::SUPPORTED_FUNCTIONS.len(),
        262
    );
    for (formula, expected) in [
        ("PERCENTILE.INC(A1:A4,0.5)", 4.),
        ("_xlfn.PERCENTILE.INC(A1:A4,0.25)", 3.25),
        ("quartile.inc(A1:A4,2.9)", 4.),
        ("QUARTILE.INC(A1:A4,4.9)", 9.),
        ("QUARTILE.INC(A1:A4,-0.9)", 1.),
        ("RANK.EQ(4,A1:A4,-1)", 2.),
        ("RANK.EQ(1,A1:A4,0.1)", 1.),
        ("RANK.EQ(4,A1:A4,0)", 2.),
        ("RANK.EQ(1,A1:A4)", 4.),
    ] {
        assert_eq!(
            value(formula, &default),
            CellValue::Error(CellError::Name),
            "{formula}"
        );
        assert_eq!(
            value(formula, &excel),
            CellValue::Number(expected),
            "{formula}"
        );
    }
    assert_eq!(value("RANK(4,A1:A4,-1)", &excel), CellValue::Number(2.));
    assert_eq!(
        value("_XLFN.QUARTILE.INC(A1:A4,2)", &excel),
        CellValue::Error(CellError::Name)
    );
}
#[test]
fn selectors_errors_lazy_branches_and_empty_samples_use_budgeted_adapter() {
    let excel = excel();
    for (formula, expected) in [
        ("QUARTILE.INC(A1:A4,-1)", CellValue::Error(CellError::Num)),
        ("QUARTILE.INC(A1:A4,5)", CellValue::Error(CellError::Num)),
        (
            "QUARTILE.INC(A1:A4,\"bad\")",
            CellValue::Error(CellError::Value),
        ),
        ("QUARTILE.INC(B1:B4,2)", CellValue::Error(CellError::Num)),
        (
            "PERCENTILE.INC(B1:B4,0.5)",
            CellValue::Error(CellError::Num),
        ),
        ("QUARTILE.INC(A1:A4,1/0)", CellValue::Error(CellError::Div0)),
        ("RANK.EQ(5,A1:A4)", CellValue::Error(CellError::NA)),
        ("RANK.EQ(4,B1:B4)", CellValue::Error(CellError::NA)),
        ("QUARTILE.INC({1,3,5},TRUE)", CellValue::Number(2.)),
        ("RANK.EQ(4,A1:A4,#REF!)", CellValue::Error(CellError::Ref)),
        ("IF(FALSE,QUARTILE.INC(1/0,5),7)", CellValue::Number(7.)),
        ("IFERROR(QUARTILE.INC(B1:B4,2),7)", CellValue::Number(7.)),
        ("QUARTILE.INC()", CellValue::Error(CellError::Value)),
    ] {
        assert_eq!(value(formula, &excel), expected, "{formula}");
    }
    for source in [
        "QUARTILE.INC(A1:A4,2)",
        "PERCENTILE.INC(A1:A4,0.5)",
        "RANK.EQ(4,A1:A4,-1)",
    ] {
        assert!(
            matches!(
                run(
                    source,
                    &excel,
                    EvaluationBudgets {
                        run_work: 20,
                        ..Default::default()
                    }
                ),
                EvaluationOutcome::Aborted { .. }
            ),
            "{source}"
        );
    }
}

#[test]
fn bound_extensions_keep_precedence_over_profile_aliases() {
    use poirot::eval::{
        EvaluationRun, EvaluationTarget, FormulaCompiler, PreparationPolicy, WorkbookView,
        functions::extensions::*, functions::registry::Arity,
    };
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "QUARTILE.INC",
            "custom-v1",
            ExtensionContract {
                arity: Arity {
                    minimum: 2,
                    maximum: 2,
                },
                evaluation: ExtensionEvaluationStrategy::Eager,
                capabilities: ExtensionCapabilities {
                    references: true,
                    arrays: true,
                    context: true,
                },
                cacheability: ExtensionCacheability::Deterministic,
            },
            |host: &mut dyn ExtensionHost| {
                host.charge_work(1)?;
                Ok(ExtensionOutput::Argument(1))
            },
        )
        .unwrap();
    let mut w = workbook();
    let target = EvaluationTarget {
        sheet: 0,
        address: CellAddress { row: 0, col: 25 },
    };
    w.sheet_mut(0)
        .unwrap()
        .set_formula(target.address, "QUARTILE.INC(A1:A4,99)")
        .unwrap();
    let policy = PreparationPolicy::with_extensions(Default::default(), &registry);
    let plan = FormulaCompiler::new().compile_workbook_with_extensions(&w, &policy, &registry);
    let configuration = excel();
    let mut run = EvaluationRun::new_with_configuration_and_extensions(
        WorkbookView::new(&w, &plan, &policy).unwrap(),
        &configuration,
        EvaluationBudgets::default(),
        &registry,
    )
    .unwrap();
    assert_eq!(run.evaluate_cell(target).unwrap(), CellValue::Number(99.));
}

#[test]
fn interest_payment_defaults_and_degenerate_cases_follow_one_contract() {
    let configuration = excel();
    for (formula, expected) in [
        ("IPMT(0,1,1,1000)", 0.0),
        ("IPMT(,1,2,1000)", 0.0),
        ("PPMT(,1,2,1000)", -500.0),
        ("IPMT(0.1,1,2,)", 0.0),
        ("PPMT(0.1,1,2,)", 0.0),
        ("IPMT(0,1,5,10000,5000)", 0.0),
        ("PPMT(0,3,5,10000,5000)", -3000.0),
        ("IPMT(0.1,1,2,0,4)", 0.0),
        ("IPMT(0.1,1,5,1000,,)", -100.0),
        ("IPMT(0.1,1,5,1000,B1,B2)", -100.0),
        ("IPMT(0.1,1,5,1000,0,1)", 0.0),
        ("IPMT(0.01,1,1.4,1000)", -10.0),
        ("IPMT(0.1,1,5,1000,\"0\",FALSE)", -100.0),
        ("IPMT(0.1,1,5,1000,{0},{0})", -100.0),
    ] {
        assert_eq!(
            value(formula, &configuration),
            CellValue::Number(expected),
            "{formula}"
        );
    }
    for formula in [
        "IPMT(0.1,1,5,1000)=IPMT(0.1,1,5,1000,0,0)",
        "PPMT(0.1,1,5,1000)=PPMT(0.1,1,5,1000,,)",
    ] {
        assert_eq!(value(formula, &configuration), CellValue::Boolean(true));
    }
    for rate in [0.0, 0.000001, -0.01, 0.1] {
        for period in [1, 3, 5] {
            for timing in [0, 1] {
                let formula = format!(
                    "IPMT({rate},{period},5,1000,200,{timing})+PPMT({rate},{period},5,1000,200,{timing})-PMT({rate},5,1000,200,{timing})"
                );
                let CellValue::Number(actual) = value(&formula, &configuration) else {
                    panic!("{formula}")
                };
                assert!(actual.abs() < 1e-10);
            }
        }
    }
    for (formula, error) in [
        ("IPMT(0,0,5,0)", CellError::Num),
        ("PPMT(0,6,5,0)", CellError::Num),
        ("IPMT(0.1,1,5,1000,0,2)", CellError::Num),
        ("IPMT(0.1,1,5,1000,0,0.5)", CellError::Num),
        ("IPMT(\"bad\",1,5,1000,1/0)", CellError::Div0),
        ("PPMT(\"bad\",1,5,1000,0,#REF!)", CellError::Ref),
        ("IPMT(0.1,1,5,1000,\"bad\",0)", CellError::Value),
    ] {
        assert_eq!(
            value(formula, &configuration),
            CellValue::Error(error),
            "{formula}"
        );
    }
}

#[test]
fn rounding_modes_and_zero_domains_follow_function_policies() {
    // Microsoft CEILING.MATH/FLOOR.MATH examples define opposite negative
    // directions for nonzero mode; Calc independently confirms fractional flags.
    let modern = excel();
    let pinned = RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi);
    for (function, default_negative, reversed_negative, positive) in
        [("CEILING.MATH", -4., -6., 6.), ("FLOOR.MATH", -6., -4., 4.)]
    {
        for significance in [-2., 2.] {
            for mode in [0., 0.1, -0.1, 1., -1., 10.] {
                let formula = format!("{function}(-5.5,{significance},{mode})");
                assert_eq!(
                    value(&formula, &modern),
                    CellValue::Number(if mode == 0. {
                        default_negative
                    } else {
                        reversed_negative
                    }),
                    "{formula}"
                );
                assert_eq!(
                    value(&formula, &pinned),
                    CellValue::Number(if mode != 0. {
                        reversed_negative
                    } else {
                        default_negative
                    }),
                    "pinned {formula}"
                );
                for (number, expected) in [(5.5, positive), (0., 0.), (-6., -6.)] {
                    let formula = format!("{function}({number},{significance},{mode})");
                    assert_eq!(
                        value(&formula, &modern),
                        CellValue::Number(expected),
                        "{formula}"
                    );
                }
            }
        }
    }
    // Mathematical kernels share the modern optional defaults and eager mode
    // conversion across profiles, retaining source-ordered spreadsheet errors.
    for configuration in [&modern, &pinned] {
        for (formula, expected) in [
            ("FLOOR.MATH(-5.5,B1,1/0)", CellValue::Error(CellError::Div0)),
            ("FLOOR.MATH(-5.5,2,B1)", CellValue::Number(-6.)),
            ("CEILING.MATH(-5.5,2,B1)", CellValue::Number(-4.)),
            ("FLOOR(B1,B1)", CellValue::Number(0.)),
            ("FLOOR(,)", CellValue::Number(0.)),
            ("CEILING.MATH(-5.5)", CellValue::Number(-5.)),
            ("FLOOR.MATH(-5.5)", CellValue::Number(-6.)),
            (
                "CEILING.MATH(-5.5,0,1/0)",
                CellValue::Error(CellError::Div0),
            ),
            ("FLOOR.MATH(-5.5,0,1/0)", CellValue::Error(CellError::Div0)),
            ("CEILING.MATH(1/0,0,1)", CellValue::Error(CellError::Div0)),
            (
                "FLOOR.MATH(0,\"bad\",1)",
                CellValue::Error(CellError::Value),
            ),
            ("FLOOR.MATH(0,1/0,NA())", CellValue::Error(CellError::Div0)),
            ("FLOOR.MATH(0,2,NA())", CellValue::Error(CellError::NA)),
            ("FLOOR(0,0)", CellValue::Number(0.)),
            ("FLOOR(0,-2)", CellValue::Number(0.)),
            ("FLOOR(-5.5,0)", CellValue::Error(CellError::Div0)),
            ("FLOOR(5.5,0)", CellValue::Error(CellError::Div0)),
            ("FLOOR(0,NA())", CellValue::Error(CellError::NA)),
            ("FLOOR(1/0,0)", CellValue::Error(CellError::Div0)),
            ("FLOOR(0,\"bad\")", CellValue::Error(CellError::Value)),
            ("FLOOR(5.5,-2)", CellValue::Error(CellError::Num)),
            ("FLOOR(-5.5,-2)", CellValue::Number(-4.)),
            ("FLOOR(-5.5,2)", CellValue::Number(-6.)),
            ("CEILING(5.5,0)", CellValue::Number(0.)),
            ("CEILING(-5.5,2)", CellValue::Number(-4.)),
            ("CEILING(-5.5,-2)", CellValue::Number(-6.)),
            ("CEILING(5.5,-2)", CellValue::Error(CellError::Num)),
            ("CEILING.PRECISE(-5.5,-2)", CellValue::Number(-4.)),
            ("FLOOR.PRECISE(-5.5,-2)", CellValue::Number(-6.)),
            ("FLOOR.MATH(1E100,1)", CellValue::Number(1e100)),
            ("FLOOR.MATH(1E-100,1E-100)", CellValue::Number(1e-100)),
        ] {
            assert_eq!(value(formula, configuration), expected, "{formula}");
        }
        for (formula, expected) in [
            ("CEILING.MATH(-5.5,,1)", -6.),
            ("FLOOR.MATH(-5.5,,1)", -5.),
            ("CEILING.MATH(-5.5,2,)", -4.),
            ("FLOOR.MATH(-5.5,2,)", -6.),
        ] {
            assert_eq!(
                value(formula, configuration),
                CellValue::Number(expected),
                "{formula}"
            );
        }
        assert!(matches!(
            run(
                "IFERROR(FLOOR.MATH(-5.5,2,1),0)",
                configuration,
                EvaluationBudgets {
                    run_work: 100,
                    ..EvaluationBudgets::default()
                }
            ),
            EvaluationOutcome::Aborted { .. }
        ));
    }
}
