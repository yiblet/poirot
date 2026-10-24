use super::*;
use crate::*;

/// Compile a deferred expression for its execution contracts.
fn expression(source: &str) -> bytecode::Compiled {
    bytecode::compile(&poirot_parsing::formula::parse_formula(source).unwrap()).unwrap()
}

/// Build the minimal workbook used by deferred expression tests.
fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
}

/// Choose the caller for deferred expression evaluation.
fn target() -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: CellAddress { row: 0, col: 0 },
    }
}

#[test]
fn separate_handles_for_one_expression_have_independent_caches() {
    let workbook = workbook();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let expression = expression("RAND()");
    let mut context = CallContext {
        session: &mut session,
        target: target(),
    };
    for _ in 0..2 {
        let mut input = context.input(&expression);
        let first = input.force().unwrap();
        let second = input.force().unwrap();
        assert_eq!(
            input.context.scalar(first).unwrap(),
            input.context.scalar(second).unwrap()
        );
    }
    assert_eq!(session.budget.usage.random_accesses[0].draws, 2);
}

#[test]
fn failed_copy_keeps_the_successfully_evaluated_input() {
    let workbook = workbook();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let expression = expression("RAND()&\"abc\"");
    let mut context = CallContext {
        session: &mut session,
        target: target(),
    };
    let mut input = context.input(&expression);
    let value = input.force().unwrap();
    let first = input.context.scalar(value).unwrap();
    let evaluated = input.context.session.budget.usage.expressions;
    input.context.session.budget.limits.text_bytes = 0;
    assert!(matches!(
        input.force(),
        Err(EvaluationFailure::Budget(BudgetFailure {
            kind: BudgetKind::TextBytes,
            ..
        }))
    ));
    input.context.session.budget.limits.text_bytes = usize::MAX;
    let value = input.force().unwrap();
    assert_eq!(input.context.scalar(value).unwrap(), first);
    assert_eq!(input.context.session.budget.usage.expressions, evaluated);
    assert_eq!(
        input.context.session.budget.usage.random_accesses[0].draws,
        1
    );
}

#[test]
fn first_force_copy_failure_retains_the_successful_evaluation() {
    let workbook = workbook();
    let expression = expression("RAND()&\"abc\"");
    // Admit exactly the evaluation work, leaving no work for the returned copy.
    // Measure this boundary separately so bytecode instruction changes do not
    // make the contract depend on a hard-coded internal operation count.
    let mut probe = Session::with_context(&workbook, EvaluationInputs::default());
    probe.budget.allocation(1).unwrap();
    probe
        .expr(
            0,
            target().address,
            ExecutableRef::Bytecode {
                program: &expression,
                start: 0,
            },
        )
        .unwrap();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.runtime.budget = BudgetMeter::new(EvaluationBudgets {
        run_work: probe.budget.usage.work_units,
        ..EvaluationBudgets::default()
    })
    .unwrap();
    let mut context = CallContext {
        session: &mut session,
        target: target(),
    };
    let mut input = context.input(&expression);
    assert!(matches!(
        input.force(),
        Err(EvaluationFailure::Budget(BudgetFailure {
            kind: BudgetKind::RunWork,
            ..
        }))
    ));
    assert!(input.value.is_some());
    let evaluated = input.context.session.budget.usage.expressions;
    input.context.session.budget.limits.run_work = u64::MAX;
    let first = input.force().unwrap();
    let second = input.force().unwrap();
    assert_eq!(
        input.context.scalar(first).unwrap(),
        input.context.scalar(second).unwrap()
    );
    assert_eq!(input.context.session.budget.usage.expressions, evaluated);
    assert_eq!(
        input.context.session.budget.usage.random_accesses[0].draws,
        1
    );
}

#[test]
fn failed_evaluation_is_retried_on_the_same_input_handle() {
    let workbook = workbook();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.budget.limits.text_bytes = 0;
    let expression = expression("RAND()&\"abc\"");
    let mut context = CallContext {
        session: &mut session,
        target: target(),
    };
    let mut input = context.input(&expression);
    assert!(matches!(
        input.force(),
        Err(EvaluationFailure::Budget(BudgetFailure {
            kind: BudgetKind::TextBytes,
            ..
        }))
    ));
    assert_eq!(
        input.context.session.budget.usage.random_accesses[0].draws,
        1
    );
    input.context.session.budget.limits.text_bytes = usize::MAX;
    let first = input.force().unwrap();
    let second = input.force().unwrap();
    assert_eq!(
        input.context.scalar(first).unwrap(),
        input.context.scalar(second).unwrap()
    );
    assert_eq!(
        input.context.session.budget.usage.random_accesses[0].draws,
        2
    );
}
