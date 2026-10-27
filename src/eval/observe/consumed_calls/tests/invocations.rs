use super::super::builtin_call::builtin_call;
use super::support::{A1, B1, Bounded, names, plan, workbook};
use crate::eval::compile::bound::Expression;
use crate::eval::config::EvaluationInputs;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::functions::registry::FunctionId;
use crate::eval::limits::{BudgetKind, EvaluationFailure};
use crate::eval::session::{FormulaExecution, Session};
use crate::model::CellValue;
use poirot_parsing::formula::Expr as Syntax;

#[test]
fn lazy_entry_order_and_semantic_errors_are_actual_consumption() {
    let workbook = workbook();
    let mut observer = Bounded::new(20, 4096);
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.ports.builtin_invocation_observer = Some(&mut observer);
    let selected = plan("IF(TRUE,ABS(-2),SUM(3))");
    assert_eq!(
        FormulaExecution::new(&mut session)
            .bound_at(
                EvaluationTarget {
                    sheet: 0,
                    address: A1
                },
                &selected
            )
            .unwrap(),
        CellValue::Number(2.)
    );
    let bad_arity = plan("ABS()");
    assert_eq!(
        FormulaExecution::new(&mut session)
            .bound_at(
                EvaluationTarget {
                    sheet: 0,
                    address: B1
                },
                &bad_arity
            )
            .unwrap(),
        CellValue::Error(crate::model::CellError::Value)
    );
    let skipped = plan("IF(TRUE,1,UNKNOWN())");
    assert_eq!(
        FormulaExecution::new(&mut session)
            .bound_at(
                EvaluationTarget {
                    sheet: 0,
                    address: A1
                },
                &skipped
            )
            .unwrap(),
        CellValue::Number(1.)
    );
    let unresolved = plan("UNKNOWN()");
    assert_eq!(
        FormulaExecution::new(&mut session)
            .bound_at(
                EvaluationTarget {
                    sheet: 0,
                    address: A1
                },
                &unresolved
            )
            .unwrap(),
        CellValue::Error(crate::model::CellError::Name)
    );
    let literal = plan("\"SUM(3)\"");
    FormulaExecution::new(&mut session)
        .bound_at(
            EvaluationTarget {
                sheet: 0,
                address: A1,
            },
            &literal,
        )
        .unwrap();
    drop(session);
    assert_eq!(names(&observer), ["IF", "ABS", "ABS", "IF"]);
    assert_eq!(
        observer
            .entries
            .iter()
            .map(|entry| entry.arity)
            .collect::<Vec<_>>(),
        [3, 1, 0, 3]
    );
    assert_eq!(observer.entries[2].target.address, B1);
}

#[test]
fn every_builtin_family_enters_the_same_typed_port() {
    let workbook = workbook();
    let mut observer = Bounded::new(20, 4096);
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.ports.builtin_invocation_observer = Some(&mut observer);
    let names = [
        "DATE", "IF", "SUM", "ROW", "VLOOKUP", "ISERROR", "ABS", "LEN", "PMT", "DEC2BIN", "MMULT",
        "SUMIF", "DCOUNT",
    ];
    for name in names {
        let syntax = Syntax::Function {
            name: name.into(),
            args: Vec::new(),
        };
        let plan = crate::eval::compile::bound::FormulaPlan::bind(&syntax).unwrap();
        let (expected, _) =
            builtin_call(&plan.expression()).expect("fixture must be typed builtin");
        assert_eq!(
            expected,
            crate::eval::functions::registry::resolve_function(
                &poirot_parsing::formula::FunctionName::from(name)
            )
            .unwrap()
            .handler
        );
        // A selected bad-arity handler is still consumed, even on a semantic error.
        FormulaExecution::new(&mut session)
            .bound_at(
                EvaluationTarget {
                    sheet: 0,
                    address: A1,
                },
                &plan,
            )
            .unwrap();
    }
    drop(session);
    assert_eq!(observer.entries.len(), 13);
    let family = |handler| match handler {
        FunctionId::Date(_) => 0,
        FunctionId::Logical(_) => 1,
        FunctionId::Statistics(_) => 2,
        FunctionId::Reference(_) => 3,
        FunctionId::Search(_) => 4,
        FunctionId::Information(_) => 5,
        FunctionId::Math(_) => 6,
        FunctionId::Text(_) => 7,
        FunctionId::Financial(_) => 8,
        FunctionId::Engineering(_) => 9,
        FunctionId::Matrix(_) => 10,
        FunctionId::Criteria(_) => 11,
        FunctionId::Database(_) => 12,
    };
    assert_eq!(
        observer
            .entries
            .iter()
            .map(|entry| family(entry.handler))
            .collect::<Vec<_>>(),
        (0..13).collect::<Vec<_>>()
    );
    for (entry, name) in observer.entries.iter().zip(names) {
        assert_eq!(
            entry.handler,
            crate::eval::functions::registry::resolve_function(
                &poirot_parsing::formula::FunctionName::from(name)
            )
            .unwrap()
            .handler
        );
        assert_eq!(entry.handler.canonical_name(), name);
        assert_eq!(entry.parsed, name);
        assert_eq!(entry.resolved, name);
        assert_eq!(entry.arity, 0);
    }
}

#[test]
fn caller_capacity_rejection_precedes_randomness_and_retains_partial_entries() {
    let workbook = workbook();
    for capacity in [0, 1] {
        let mut observer = Bounded::new(capacity, 4096);
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.ports.builtin_invocation_observer = Some(&mut observer);
        let formula = plan("IF(TRUE,RAND(),0)");
        let failure = FormulaExecution::new(&mut session)
            .bound_at(
                EvaluationTarget {
                    sheet: 0,
                    address: A1,
                },
                &formula,
            )
            .unwrap_err();
        assert_eq!(failure.budget().unwrap().kind, BudgetKind::Materialization);
        assert!(session.state.runtime.effects.random_streams.is_empty());
        assert!(
            session
                .state
                .runtime
                .budget
                .usage
                .random_accesses
                .is_empty()
        );
        drop(session);
        assert_eq!(observer.entries.len(), capacity);
        if capacity == 1 {
            assert_eq!(names(&observer), ["IF"]);
        }
    }
}

#[test]
fn nested_dependency_target_and_memo_hits_are_not_probe_target_or_new_calls() {
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(A1, "ABS(-2)")
        .unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(B1, "A1+A1")
        .unwrap();
    let policy = crate::eval::compile::compiler::PreparationPolicy::default();
    let mut compiler = crate::eval::compile::compiler::FormulaCompiler::new();
    let compiled = compiler.compile_workbook(&workbook, &policy);
    let mut observer = Bounded::new(20, 4096);
    let mut run = crate::eval::run::EvaluationRun::new(
        crate::eval::run::WorkbookView::new(&workbook, &compiled, &policy).unwrap(),
        EvaluationInputs::default(),
        crate::eval::limits::EvaluationBudgets::default(),
    )
    .unwrap()
    .with_builtin_invocation_observer(&mut observer);
    let target = EvaluationTarget {
        sheet: 0,
        address: B1,
    };
    assert_eq!(run.evaluate_cell(target).unwrap(), CellValue::Number(4.));
    assert_eq!(run.evaluate_cell(target).unwrap(), CellValue::Number(4.));
    drop(run);
    assert_eq!(names(&observer), ["ABS"]);
    assert_eq!(observer.entries[0].target.address, A1);
}

#[test]
fn exact_alias_lookup_metadata_and_stale_namespace_are_preserved() {
    let workbook = workbook();
    let syntax = Syntax::Function {
        name: "_xlfn.IfNa".into(),
        args: vec![Syntax::Number(2.), Syntax::Number(3.)],
    };
    let mut registry = crate::eval::functions::extensions::ExtensionRegistry::new();
    registry
        .set_external_namespace(Some(
            crate::eval::functions::external_namespace::ExternalFunctionNamespace::from_stored_names(
                ["_xlfn.IFNA", "_XLFN.ifna"],
                2,
            )
            .unwrap(),
        ))
        .unwrap();
    let first =
        crate::eval::compile::bound::FormulaPlan::bind_with_extensions(&syntax, &registry).unwrap();
    let mut observer = Bounded::new(20, 4096);
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.sources.extensions = Some(&registry);
    session.ports.builtin_invocation_observer = Some(&mut observer);
    assert_eq!(
        FormulaExecution::new(&mut session)
            .bound_at(
                EvaluationTarget {
                    sheet: 0,
                    address: A1
                },
                &first
            )
            .unwrap(),
        CellValue::Number(2.)
    );
    drop(session);
    assert_eq!(observer.entries[0].parsed, "_xlfn.IfNa");
    assert_eq!(observer.entries[0].resolved, "_xlfn.IFNA");
    assert_eq!(observer.entries[0].handler.canonical_name(), "IFNA");
    registry
        .set_external_namespace(Some(
            crate::eval::functions::external_namespace::ExternalFunctionNamespace::from_stored_names(
                ["_XLFN.ifna", "_xlfn.IFNA"],
                2,
            )
            .unwrap(),
        ))
        .unwrap();
    let second =
        crate::eval::compile::bound::FormulaPlan::bind_with_extensions(&syntax, &registry).unwrap();
    assert_ne!(first.extension_identity(), second.extension_identity());
    assert!(matches!(
        second.expression(),
        Expression::UnresolvedFunction(_)
    ));
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.sources.extensions = Some(&registry);
    session.ports.builtin_invocation_observer = Some(&mut observer);
    assert_eq!(
        FormulaExecution::new(&mut session).bound_at(
            EvaluationTarget {
                sheet: 0,
                address: A1
            },
            &first
        ),
        Err(EvaluationFailure::StalePlan)
    );
    assert_eq!(
        FormulaExecution::new(&mut session)
            .bound_at(
                EvaluationTarget {
                    sheet: 0,
                    address: A1
                },
                &second
            )
            .unwrap(),
        CellValue::Error(crate::model::CellError::Name)
    );
    drop(session);
    assert_eq!(observer.entries.len(), 1);
    for spelling in ["IFNA", "_xlfn.IFNA", "_xlfn.IfNa"] {
        let syntax = Syntax::Function {
            name: spelling.into(),
            args: vec![Syntax::Number(2.), Syntax::Number(3.)],
        };
        let bound = crate::eval::compile::bound::FormulaPlan::bind(&syntax).unwrap();
        let expression = bound.expression();
        let (handler, arguments) = builtin_call(&expression).unwrap();
        assert_eq!(handler.canonical_name(), "IFNA");
        assert_eq!(arguments.source_name, spelling);
        assert_eq!(arguments.resolved_spelling(), spelling);
        assert!(arguments.resolved_name.is_none());
    }
    let upper = Syntax::Function {
        name: "_XLFN.IFNA".into(),
        args: Vec::new(),
    };
    assert!(
        builtin_call(
            &crate::eval::compile::bound::FormulaPlan::bind(&upper)
                .unwrap()
                .expression()
        )
        .is_none()
    );
}

#[test]
fn extension_execution_and_external_cache_values_do_not_fabricate_builtin_entries() {
    use crate::eval::functions::extensions::*;
    /// Extension provider used to distinguish extension calls from builtin observations.
    struct Provider;
    impl ExtensionFunction for Provider {
        fn evaluate(&self, _host: &mut dyn ExtensionHost) -> ExtensionResult<ExtensionOutput> {
            Ok(ExtensionOutput::Scalar(CellValue::Number(7.)))
        }
    }
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(A1, "ABS(-2)")
        .unwrap();
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "CUSTOM",
            "test-consumed-custom-v1",
            ExtensionContract {
                arity: crate::eval::functions::registry::Arity {
                    minimum: 0,
                    maximum: 0,
                },
                evaluation: ExtensionEvaluationStrategy::Eager,
                capabilities: ExtensionCapabilities {
                    references: false,
                    arrays: false,
                    context: false,
                },
                cacheability: ExtensionCacheability::Deterministic,
            },
            Provider,
        )
        .unwrap();
    let extension = crate::eval::compile::bound::FormulaPlan::bind_with_extensions(
        &Syntax::Function {
            name: "CUSTOM".into(),
            args: Vec::new(),
        },
        &registry,
    )
    .unwrap();
    let mut observer = Bounded::new(20, 4096);
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.sources.extensions = Some(&registry);
    session.ports.builtin_invocation_observer = Some(&mut observer);
    assert_eq!(
        FormulaExecution::new(&mut session)
            .bound_at(
                EvaluationTarget {
                    sheet: 0,
                    address: B1
                },
                &extension
            )
            .unwrap(),
        CellValue::Number(7.)
    );
    assert_eq!(
        session
            .state
            .runtime
            .effects
            .extension_trace
            .invocations
            .len(),
        1
    );
    drop(session);
    assert!(observer.entries.is_empty());
    /// Supplies a cached result to verify reuse fabricates no builtin handler entry.
    struct Cache;
    impl crate::eval::ports::CachedCellReader for Cache {
        fn cached_cell(
            &mut self,
            _target: EvaluationTarget,
        ) -> Result<Option<std::sync::Arc<CellValue>>, EvaluationFailure> {
            Ok(Some(std::sync::Arc::new(CellValue::Number(19.))))
        }
    }
    let policy = crate::eval::compile::compiler::PreparationPolicy::default();
    let compiled =
        crate::eval::compile::compiler::FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let mut cache = Cache;
    let mut run = crate::eval::run::EvaluationRun::new(
        crate::eval::run::WorkbookView::new(&workbook, &compiled, &policy).unwrap(),
        EvaluationInputs::default(),
        crate::eval::limits::EvaluationBudgets::default(),
    )
    .unwrap()
    .with_builtin_invocation_observer(&mut observer)
    .with_cell_reader(&mut cache);
    assert_eq!(
        run.evaluate_cell(EvaluationTarget {
            sheet: 0,
            address: A1
        })
        .unwrap(),
        CellValue::Number(19.)
    );
    drop(run);
    assert!(observer.entries.is_empty());
}

#[test]
fn aborted_run_retains_partial_trace_without_complete_semantic_result() {
    let workbook = workbook();
    let policy = crate::eval::compile::compiler::PreparationPolicy::default();
    let compiled =
        crate::eval::compile::compiler::FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let formula = plan("IF(TRUE,ABS(-2),0)");
    let mut observer = Bounded::new(1, 4096);
    let mut run = crate::eval::run::EvaluationRun::new(
        crate::eval::run::WorkbookView::new(&workbook, &compiled, &policy).unwrap(),
        EvaluationInputs::default(),
        crate::eval::limits::EvaluationBudgets::default(),
    )
    .unwrap()
    .with_builtin_invocation_observer(&mut observer);
    let result = run.evaluate_bound(
        EvaluationTarget {
            sheet: 0,
            address: A1,
        },
        &formula,
    );
    let report = run.finish(result);
    match report.outcome {
        crate::eval::limits::EvaluationOutcome::Aborted { failure, .. } => {
            assert_eq!(failure.budget().unwrap().kind, BudgetKind::Materialization);
            assert_eq!(
                failure.budget().unwrap().target,
                Some(EvaluationTarget {
                    sheet: 0,
                    address: A1
                })
            );
        }
        crate::eval::limits::EvaluationOutcome::Complete { .. } => {
            panic!("partial trace cannot establish a complete run")
        }
    }
    assert_eq!(names(&observer), ["IF"]);
}
