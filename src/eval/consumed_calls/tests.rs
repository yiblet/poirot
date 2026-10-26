use super::*;
use crate::eval::{BudgetFailure, BudgetKind, EvaluationInputs, Session};
use crate::model::{CellAddress, CellValue, Workbook};
use poirot_parsing::formula::Expr as Syntax;

/// Owned builtin entry evidence used to compare observed call identities.
#[derive(Debug, PartialEq, Eq)]
struct Entry {
    target: EvaluationTarget,
    handler: FunctionId,
    parsed: String,
    resolved: String,
    arity: usize,
}
/// Test invocation observer admitting entry and text capacity before copying events.
struct Bounded {
    entries: Vec<Entry>,
    maximum_entries: usize,
    maximum_bytes: usize,
    retained_bytes: usize,
}
impl Bounded {
    fn new(maximum_entries: usize, maximum_bytes: usize) -> Self {
        Self {
            entries: Vec::new(),
            maximum_entries,
            maximum_bytes,
            retained_bytes: 0,
        }
    }
}
impl BuiltinInvocationObserver for Bounded {
    fn consumed(&mut self, call: ConsumedBuiltinInvocation<'_>) -> Result<(), EvaluationFailure> {
        let bytes = call
            .parsed_spelling
            .len()
            .saturating_add(call.resolved_spelling.len());
        let (kind, limit, consumed, requested) = if self.entries.len() >= self.maximum_entries {
            (
                BudgetKind::Materialization,
                self.maximum_entries,
                self.entries.len(),
                1,
            )
        } else if bytes > self.maximum_bytes.saturating_sub(self.retained_bytes) {
            (
                BudgetKind::TextBytes,
                self.maximum_bytes,
                self.retained_bytes,
                bytes,
            )
        } else {
            // Capacity is checked before any allocation/copy.
            self.entries.push(Entry {
                target: call.target,
                handler: call.handler,
                parsed: call.parsed_spelling.to_owned(),
                resolved: call.resolved_spelling.to_owned(),
                arity: call.argument_count,
            });
            self.retained_bytes += bytes;
            return Ok(());
        };
        Err(EvaluationFailure::Budget(BudgetFailure {
            kind,
            limit: limit as u64,
            consumed: consumed as u64,
            requested: requested as u64,
            target: Some(call.target),
        }))
    }
}
const A1: CellAddress = CellAddress { row: 0, col: 0 };
const B1: CellAddress = CellAddress { row: 0, col: 1 };
fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
}
fn plan(source: &str) -> crate::eval::bound::FormulaPlan {
    crate::eval::bound::FormulaPlan::bind(&poirot_parsing::formula::parse_formula(source).unwrap())
        .unwrap()
}
fn names(observer: &Bounded) -> Vec<&str> {
    observer
        .entries
        .iter()
        .map(|entry| entry.handler.canonical_name())
        .collect()
}
#[test]
fn lazy_entry_order_and_semantic_errors_are_actual_consumption() {
    let workbook = workbook();
    let mut observer = Bounded::new(20, 4096);
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.builtin_invocation_observer = Some(&mut observer);
    let selected = plan("IF(TRUE,ABS(-2),SUM(3))");
    assert_eq!(
        session
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
        session
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
        session
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
        session
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
    session
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
    session.builtin_invocation_observer = Some(&mut observer);
    let names = [
        "DATE", "IF", "SUM", "ROW", "VLOOKUP", "ISERROR", "ABS", "LEN", "PMT", "DEC2BIN", "MMULT",
        "SUMIF", "DCOUNT",
    ];
    for name in names {
        let syntax = Syntax::Function {
            name: name.into(),
            args: Vec::new(),
        };
        let plan = crate::eval::bound::FormulaPlan::bind(&syntax).unwrap();
        let (expected, _) =
            builtin_call(&plan.expression()).expect("fixture must be typed builtin");
        assert_eq!(
            expected,
            crate::eval::registry::resolve_function(&poirot_parsing::formula::FunctionName::from(
                name
            ))
            .unwrap()
            .handler
        );
        // A selected bad-arity handler is still consumed, even on a semantic error.
        session
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
            crate::eval::registry::resolve_function(&poirot_parsing::formula::FunctionName::from(
                name
            ))
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
        session.builtin_invocation_observer = Some(&mut observer);
        let formula = plan("IF(TRUE,RAND(),0)");
        let failure = session
            .bound_at(
                EvaluationTarget {
                    sheet: 0,
                    address: A1,
                },
                &formula,
            )
            .unwrap_err();
        assert_eq!(failure.budget().unwrap().kind, BudgetKind::Materialization);
        assert!(session.random_streams.is_empty());
        assert!(session.budget.usage.random_accesses.is_empty());
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
    let policy = crate::eval::PreparationPolicy::default();
    let mut compiler = crate::eval::FormulaCompiler::new();
    let compiled = compiler.compile_workbook(&workbook, &policy);
    let mut observer = Bounded::new(20, 4096);
    let mut run = crate::eval::EvaluationRun::new(
        crate::eval::WorkbookView::new(&workbook, &compiled, &policy).unwrap(),
        EvaluationInputs::default(),
        crate::eval::EvaluationBudgets::default(),
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
    let mut registry = crate::eval::extensions::ExtensionRegistry::new();
    registry
        .set_external_namespace(Some(
            crate::eval::external_namespace::ExternalFunctionNamespace::from_stored_names(
                ["_xlfn.IFNA", "_XLFN.ifna"],
                2,
            )
            .unwrap(),
        ))
        .unwrap();
    let first = crate::eval::bound::FormulaPlan::bind_with_extensions(&syntax, &registry).unwrap();
    let mut observer = Bounded::new(20, 4096);
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.extensions = Some(&registry);
    session.builtin_invocation_observer = Some(&mut observer);
    assert_eq!(
        session
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
            crate::eval::external_namespace::ExternalFunctionNamespace::from_stored_names(
                ["_XLFN.ifna", "_xlfn.IFNA"],
                2,
            )
            .unwrap(),
        ))
        .unwrap();
    let second = crate::eval::bound::FormulaPlan::bind_with_extensions(&syntax, &registry).unwrap();
    assert_ne!(first.extension_identity(), second.extension_identity());
    assert!(matches!(
        second.expression(),
        Expression::UnresolvedFunction(_)
    ));
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.extensions = Some(&registry);
    session.builtin_invocation_observer = Some(&mut observer);
    assert_eq!(
        session.bound_at(
            EvaluationTarget {
                sheet: 0,
                address: A1
            },
            &first
        ),
        Err(EvaluationFailure::StalePlan)
    );
    assert_eq!(
        session
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
        let bound = crate::eval::bound::FormulaPlan::bind(&syntax).unwrap();
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
            &crate::eval::bound::FormulaPlan::bind(&upper)
                .unwrap()
                .expression()
        )
        .is_none()
    );
}
#[test]
fn extension_execution_and_external_cache_values_do_not_fabricate_builtin_entries() {
    use crate::eval::extensions::*;
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
                arity: crate::eval::registry::Arity {
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
    let extension = crate::eval::bound::FormulaPlan::bind_with_extensions(
        &Syntax::Function {
            name: "CUSTOM".into(),
            args: Vec::new(),
        },
        &registry,
    )
    .unwrap();
    let mut observer = Bounded::new(20, 4096);
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.extensions = Some(&registry);
    session.builtin_invocation_observer = Some(&mut observer);
    assert_eq!(
        session
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
    assert_eq!(session.extension_trace.invocations.len(), 1);
    drop(session);
    assert!(observer.entries.is_empty());
    /// Supplies a cached result to verify reuse fabricates no builtin handler entry.
    struct Cache;
    impl crate::eval::CachedCellReader for Cache {
        fn cached_cell(
            &mut self,
            _target: EvaluationTarget,
        ) -> Result<Option<std::sync::Arc<CellValue>>, EvaluationFailure> {
            Ok(Some(std::sync::Arc::new(CellValue::Number(19.))))
        }
    }
    let policy = crate::eval::PreparationPolicy::default();
    let compiled = crate::eval::FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let mut cache = Cache;
    let mut run = crate::eval::EvaluationRun::new(
        crate::eval::WorkbookView::new(&workbook, &compiled, &policy).unwrap(),
        EvaluationInputs::default(),
        crate::eval::EvaluationBudgets::default(),
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
    let policy = crate::eval::PreparationPolicy::default();
    let compiled = crate::eval::FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let formula = plan("IF(TRUE,ABS(-2),0)");
    let mut observer = Bounded::new(1, 4096);
    let mut run = crate::eval::EvaluationRun::new(
        crate::eval::WorkbookView::new(&workbook, &compiled, &policy).unwrap(),
        EvaluationInputs::default(),
        crate::eval::EvaluationBudgets::default(),
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
        crate::eval::EvaluationOutcome::Aborted { failure, .. } => {
            assert_eq!(failure.budget().unwrap().kind, BudgetKind::Materialization);
            assert_eq!(
                failure.budget().unwrap().target,
                Some(EvaluationTarget {
                    sheet: 0,
                    address: A1
                })
            );
        }
        crate::eval::EvaluationOutcome::Complete { .. } => {
            panic!("partial trace cannot establish a complete run")
        }
    }
    assert_eq!(names(&observer), ["IF"]);
}

#[path = "tests/statistical_aliases.rs"]
mod statistical_aliases;

#[path = "tests/observer_cost.rs"]
mod observer_cost;
