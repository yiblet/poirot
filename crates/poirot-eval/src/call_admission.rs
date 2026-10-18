//! Resolved builtin call admission shared by expression, VM, and direct adapter entry.
//! The two entry orders are explicit because observer refusal changes which reads occur.
use crate::*;

/// Resolved invocation metadata and policy consumption needed before handler entry.
#[derive(Clone, Copy)]
pub(crate) struct ResolvedCall<'a> {
    /// Actual builtin invocation, absent for unresolved calls.
    pub invocation: Option<ConsumedBuiltinInvocation<'a>>,
    /// Whether name resolution depended on the selected builtin policy.
    pub read_builtin_policy: bool,
}

/// Observable ordering of invocation admission and policy dependency reads.
#[derive(Clone, Copy)]
pub(crate) enum AdmissionOrder {
    /// Tree/VM: a refused invocation consumes no policy or context reads.
    ObserverBeforePolicy,
    /// Public direct calls: profile/unknown lookup consumes policy before observation.
    PolicyBeforeObserver,
}

impl Session<'_> {
    /// Observe and admit a call before recording its selected-handler context guards.
    pub(crate) fn admit_resolved_call(
        &mut self,
        call: ResolvedCall<'_>,
        order: AdmissionOrder,
    ) -> EvalResult<()> {
        match order {
            AdmissionOrder::ObserverBeforePolicy => {
                self.observe_resolved_call(call)?;
                if call.read_builtin_policy {
                    self.read(DependencyRead::BuiltinPolicy);
                }
            }
            AdmissionOrder::PolicyBeforeObserver => {
                if call.read_builtin_policy {
                    self.read(DependencyRead::BuiltinPolicy);
                }
                self.observe_resolved_call(call)?;
            }
        }
        if let Some(invocation) = call.invocation {
            self.read_builtin_context(invocation.handler);
        }
        Ok(())
    }

    fn observe_resolved_call(&mut self, call: ResolvedCall<'_>) -> EvalResult<()> {
        if let (Some(observer), Some(invocation)) =
            (&mut self.builtin_invocation_observer, call.invocation)
        {
            observer.consumed(invocation)?;
        }
        Ok(())
    }

    /// Record conservative context categories for an actually selected builtin.
    pub(crate) fn read_builtin_context(&mut self, handler: FunctionId) {
        if !matches!(handler, FunctionId::Date(_)) {
            let dependencies = handler.descriptor().dependencies;
            let mut context: Option<DependencyPolicy> = None;
            for field in [
                DependencyPolicy::TIME,
                DependencyPolicy::LOCALIZATION,
                DependencyPolicy::DATE_SYSTEM,
                DependencyPolicy::POSITION,
            ] {
                if dependencies.contains(field) {
                    context = Some(context.map_or(field, |previous| previous.union(field)));
                }
            }
            if let Some(context) = context {
                self.read(DependencyRead::Context(context));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{CallContext, FunctionAdapter, FunctionValue, builtins};

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
        let bound = bound::FormulaPlan::bind(&syntax).unwrap();
        let compiled = bytecode::compile(&syntax).unwrap();
        for vm in [false, true] {
            let mut reads = Reads::default();
            let mut refuse = Refuse;
            let mut session = Session::with_context(&workbook, EvaluationInputs::default());
            session.builtin_policy = BuiltinSemanticsPolicy::ExcelDocumentation;
            session.dependency_sink = Some(&mut reads);
            session.builtin_invocation_observer = Some(&mut refuse);
            let result = if vm {
                session.bytecode_expr(0, target().address, &compiled)
            } else {
                session.bound_at(target(), &bound).map(Value::Scalar)
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

        let policy = EvaluationPolicy::default();
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
        run.session.builtin_policy = BuiltinSemanticsPolicy::ExcelDocumentation;
        let result = run.call_with_adapter(
            &builtins(),
            target(),
            &poirot_parsing::formula::FunctionName::from("RANK.EQ"),
            [
                bytecode::compile(&poirot_parsing::formula::Expr::Number(4.)).unwrap(),
                bytecode::compile(&poirot_parsing::formula::Expr::Number(1.)).unwrap(),
                bytecode::compile(&poirot_parsing::formula::Expr::Number(0.)).unwrap(),
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
        session.dependency_sink = Some(&mut reads);
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
}
