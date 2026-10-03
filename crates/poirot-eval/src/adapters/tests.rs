use super::*;
use crate::*;

fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
}
fn target() -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: CellAddress { row: 0, col: 0 },
    }
}
fn expression(source: &str) -> bytecode::Compiled {
    bytecode::compile(&poirot_formula::parse_formula(source).unwrap()).unwrap()
}
fn call(
    adapter: &impl FunctionAdapter,
    workbook: &Workbook,
    name: &str,
    parameters: &[&str],
) -> CellValue {
    let policy = EvaluationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(workbook, &policy);
    let view = WorkbookView::new(workbook, &plan, &policy).unwrap();
    let mut run = EvaluationRun::new(
        view,
        EvaluationInputs::default(),
        EvaluationBudgets::default(),
    )
    .unwrap();
    run.call_with_adapter(
        adapter,
        target(),
        &name.into(),
        parameters.iter().map(|source| expression(source)).collect(),
    )
    .unwrap()
}

#[test]
fn composed_names_preserve_every_catalog_entry_and_alias() {
    let adapters = builtins();
    let mut names: Vec<_> = adapters.functions().map(|entry| entry.name).collect();
    names.sort_unstable();
    let mut expected: Vec<_> = FUNCTION_DESCRIPTORS
        .iter()
        .map(|entry| entry.name)
        .collect();
    expected.sort_unstable();
    assert_eq!(names, expected);
    for descriptor in FUNCTION_DESCRIPTORS {
        assert_eq!(adapters.resolve(&descriptor.name.into()), Some(descriptor));
    }
    assert_eq!(
        adapters.resolve(&"_xlfn.IFNA".into()),
        adapters.resolve(&"IFNA".into())
    );
    assert!(adapters.resolve(&"_XLFN.IFNA".into()).is_none());
    assert!(adapters.resolve(&"_xlfn.SUM".into()).is_none());
}

#[test]
fn families_and_nested_composition_share_the_call_interface() {
    let workbook = workbook();
    let adapters = MathAdapter.chain(FinancialAdapter.chain(LogicalAdapter));
    assert_eq!(
        call(&MathAdapter, &workbook, "fact", &["5"]),
        CellValue::Number(120.)
    );
    assert_eq!(
        call(&adapters, &workbook, "FV", &["0", "2", "-10"]),
        CellValue::Number(20.)
    );
    assert_eq!(
        call(&adapters, &workbook, "IF", &["TRUE", "7", "1/0"]),
        CellValue::Number(7.)
    );
    assert_eq!(
        call(&adapters, &workbook, "LEN", &["1/0"]),
        CellValue::Error(CellError::Name)
    );
}

#[test]
fn direct_calls_preserve_references_missing_values_and_short_circuiting() {
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(CellAddress { row: 1, col: 0 }, 42.)
        .unwrap();
    assert_eq!(
        call(&builtins(), &workbook, "IF", &["TRUE", "A2", "1/0"]),
        CellValue::Number(42.)
    );
    assert_eq!(
        call(&builtins(), &workbook, "ISREF", &["A2"]),
        CellValue::Boolean(true)
    );
    assert_eq!(
        call(
            &builtins(),
            &workbook,
            "SUM",
            &["1", "2", "3", "4", "5", "6"]
        ),
        CellValue::Number(21.)
    );
    assert_eq!(
        call(
            &builtins(),
            &workbook,
            "IF",
            &["TRUE", "IF(TRUE,,1)", "1/0"]
        ),
        CellValue::Number(0.)
    );
}

#[test]
fn inline_parameters_spill_without_an_arity_limit() {
    let parameters: ParamVec<_> = (0..4).collect();
    assert!(!parameters.spilled());
    let parameters: ParamVec<_> = (0..255).collect();
    assert!(parameters.spilled());
    assert_eq!(parameters.len(), 255);
}

#[test]
fn forcing_a_deferred_input_once_does_not_repeat_randomness() {
    let workbook = workbook();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let expression = expression("RAND()");
    let mut context = CallContext {
        session: &mut session,
        target: target(),
    };
    let (first, second) = {
        let mut input = context.input(&expression);
        (input.force().unwrap(), input.force().unwrap())
    };
    assert_eq!(
        context.scalar(first).unwrap(),
        context.scalar(second).unwrap()
    );
    assert_eq!(session.budget.usage.random_accesses[0].draws, 1);
}

#[test]
fn direct_call_keeps_operational_failures_separate_and_restores_lifecycle() {
    let workbook = workbook();
    let policy = EvaluationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let view = WorkbookView::new(&workbook, &plan, &policy).unwrap();
    let mut run = EvaluationRun::new(
        view,
        EvaluationInputs::default(),
        EvaluationBudgets::default(),
    )
    .unwrap();
    assert!(matches!(
        run.call(
            target(),
            &"DOLLAR".into(),
            [expression("1")].into_iter().collect()
        ),
        Err(EvaluationFailure::UnsupportedCapability {
            capability: CapabilityId::Formatting,
            ..
        })
    ));
    assert!(run.session.consumer.is_none());
    assert_eq!(
        run.call(
            target(),
            &"FACT".into(),
            [expression("4")].into_iter().collect()
        )
        .unwrap(),
        CellValue::Number(24.)
    );
}

#[test]
fn distinct_random_arguments_and_later_invocations_draw_independently() {
    let workbook = workbook();
    let policy = EvaluationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let view = WorkbookView::new(&workbook, &plan, &policy).unwrap();
    let mut run = EvaluationRun::new(
        view,
        EvaluationInputs::default(),
        EvaluationBudgets::default(),
    )
    .unwrap();
    for expected_draws in [2, 4] {
        run.call(
            target(),
            &"SUM".into(),
            [expression("RAND()"), expression("RAND()")]
                .into_iter()
                .collect(),
        )
        .unwrap();
        assert_eq!(
            run.session.budget.usage.random_accesses[0].draws,
            expected_draws
        );
    }
    run.call(
        target(),
        &"IF".into(),
        [expression("TRUE"), expression("1"), expression("RAND()")]
            .into_iter()
            .collect(),
    )
    .unwrap();
    assert_eq!(run.session.budget.usage.random_accesses[0].draws, 4);
}

#[test]
fn deferred_input_keeps_missing_distinct_from_a_blank_value() {
    let workbook = workbook();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut context = CallContext {
        session: &mut session,
        target: target(),
    };
    let expression = bytecode::compile(&poirot_formula::Expr::Missing).unwrap();
    assert!(context.input(&expression).force().unwrap().is_missing());
}

#[test]
fn borrowed_bytecode_arguments_force_once_and_skip_other_blocks() {
    let workbook = workbook();
    let syntax = poirot_formula::parse_formula("IF(TRUE,RAND(),1/0)").unwrap();
    let program = crate::bytecode::compile(&syntax).unwrap();
    let (ip, invoke) = program
        .template
        .code
        .iter()
        .enumerate()
        .find(|(_, instruction)| instruction.opcode == crate::bytecode::Opcode::Invoke)
        .unwrap();
    let parameters = FunctionParameters(crate::ArgumentsRef::Bytecode {
        program: &program,
        first: ip + 2,
        count: invoke.b as usize,
    });
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut context = CallContext {
        session: &mut session,
        target: target(),
    };
    let (first, repeated) = {
        let mut input = context.argument(parameters, 1).unwrap();
        (input.force().unwrap(), input.force().unwrap())
    };
    assert_eq!(
        context.scalar(first).unwrap(),
        context.scalar(repeated).unwrap()
    );
    assert_eq!(session.budget.usage.random_accesses[0].draws, 1);
    assert!(session.bytecode_stacks.iter().all(Vec::is_empty));
}
