use poirot_eval::{
    BuiltinSemanticsPolicy, EvaluationBudgets, EvaluationOutcome, RunConfiguration,
    run_formula_with_configuration,
};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};

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
fn documented_aliases_are_opt_in_and_do_not_change_default_registry() {
    let default = RunConfiguration::default();
    let excel = excel();
    assert_eq!(default.builtin_policy(), BuiltinSemanticsPolicy::PinnedPoi);
    assert_ne!(
        default.fingerprint().computation(),
        excel.fingerprint().computation()
    );
    assert_eq!(poirot_eval::registry::SUPPORTED_FUNCTIONS.len(), 256);
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
        value("RANK(4,A1:A4,-1)", &default),
        CellValue::Error(CellError::Num)
    );
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
    use poirot_eval::{
        EvaluationPolicy, EvaluationRun, EvaluationTarget, FormulaCompiler, WorkbookView,
        extensions::*, registry::Arity,
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
        .set_formula(target.address, "QUARTILE.INC(A1:A4,99)", CellValue::Blank)
        .unwrap();
    let policy = EvaluationPolicy::with_extensions(Default::default(), &registry);
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
