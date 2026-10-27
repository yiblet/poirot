use crate::eval::adapters::{CallContext, FunctionAdapter, FunctionValue, builtins};
use crate::eval::compile;
use crate::eval::compile::compiler::{FormulaCompiler, PreparationPolicy};
use crate::eval::config::{BuiltinSemanticsPolicy, EvaluationInputs};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationBudgets, EvaluationFailure};
use crate::eval::observe::{BuiltinInvocationObserver, ConsumedBuiltinInvocation};
use crate::eval::ports::{DependencyRead, DependencyReadSink, ReadEvent};
use crate::eval::run::{EvaluationRun, WorkbookView};
use crate::eval::session::{FormulaExecution, Session};
use crate::eval::values::Value;
use crate::eval::vm::bytecode_vm::Vm;
use crate::model::{CellAddress, CellError, CellValue, Workbook};

/// Test observer rejecting every builtin before handler effects occur.
struct Refuse;

impl BuiltinInvocationObserver for Refuse {
    fn consumed(&mut self, _: ConsumedBuiltinInvocation<'_>) -> EvalResult<()> {
        Err(EvaluationFailure::UnsupportedCapability {
            capability: CapabilityId::FunctionAdapter,
            source: None,
            detail: "observer refused invocation".into(),
        })
    }
}

/// Collects reads to assert admission and policy ordering.
#[derive(Default)]
struct Reads(Vec<ReadEvent>);

impl DependencyReadSink for Reads {
    fn read(&mut self, event: ReadEvent) {
        self.0.push(event);
    }
}

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

#[test]
fn refusing_profile_observer_preserves_tree_vm_and_direct_policy_reads() {
    let workbook = workbook();
    let syntax = poirot_parsing::formula::parse_formula("RANK.EQ(4,{1,4,4},0)").unwrap();
    let bound = compile::bound::FormulaPlan::bind(&syntax).unwrap();
    let compiled = compile::bytecode::compile(&syntax).unwrap();
    for vm in [false, true] {
        let mut reads = Reads::default();
        let mut refuse = Refuse;
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.policies.builtin_policy = BuiltinSemanticsPolicy::ExcelDocumentation;
        session.ports.dependency_sink = Some(&mut reads);
        session.ports.builtin_invocation_observer = Some(&mut refuse);
        let result = if vm {
            Vm::new(&mut session).run(0, target().address, &compiled)
        } else {
            FormulaExecution::new(&mut session)
                .bound_at(target(), &bound)
                .map(Value::Scalar)
        };
        assert!(matches!(
            result,
            Err(EvaluationFailure::UnsupportedCapability { .. })
        ));
        drop(session);
        assert!(
            reads
                .0
                .iter()
                .all(|event| event.read != DependencyRead::BuiltinPolicy)
        );
    }

    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let view = WorkbookView::new(&workbook, &plan, &policy).unwrap();
    let mut reads = Reads::default();
    let mut refuse = Refuse;
    let mut run = EvaluationRun::new(
        view,
        EvaluationInputs::default(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_dependency_sink(&mut reads)
    .with_builtin_invocation_observer(&mut refuse);
    run.session.policies.builtin_policy = BuiltinSemanticsPolicy::ExcelDocumentation;
    let result = run.call_with_adapter(
        &builtins(),
        target(),
        &poirot_parsing::formula::FunctionName::from("RANK.EQ"),
        [
            compile::bytecode::compile(&poirot_parsing::formula::Expr::Number(4.)).unwrap(),
            compile::bytecode::compile(&poirot_parsing::formula::Expr::Number(1.)).unwrap(),
            compile::bytecode::compile(&poirot_parsing::formula::Expr::Number(0.)).unwrap(),
        ]
        .into_iter()
        .collect(),
    );
    assert!(matches!(
        result,
        Err(EvaluationFailure::UnsupportedCapability { .. })
    ));
    drop(run);
    assert_eq!(
        reads
            .0
            .iter()
            .filter(|event| event.read == DependencyRead::BuiltinPolicy)
            .count(),
        1,
    );
}

#[test]
fn unknown_direct_registration_reads_policy_then_opaque_provider() {
    let workbook = workbook();
    let mut reads = Reads::default();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.ports.dependency_sink = Some(&mut reads);
    let value: FunctionValue = {
        let mut context = CallContext {
            session: &mut session,
            target: target(),
        };
        builtins()
            .call(
                &mut context,
                &poirot_parsing::formula::FunctionName::from("UNKNOWN"),
                std::iter::empty().collect(),
            )
            .unwrap()
    };
    assert!(matches!(
        value.0,
        Value::Scalar(CellValue::Error(CellError::Name))
    ));
    drop(session);
    assert_eq!(
        reads.0.iter().map(|event| &event.read).collect::<Vec<_>>(),
        vec![
            &DependencyRead::BuiltinPolicy,
            &DependencyRead::OpaqueProvider("UNKNOWN".into()),
        ]
    );
}
