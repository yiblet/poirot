use super::super::bytecode_vm::{decode_binary, decode_unary};
use crate::eval::compile::bound::Expression;
use crate::eval::compile::bytecode::{Compiled, OperandCursor};
use crate::eval::config::{ArrayResultPolicy, BuiltinSemanticsPolicy, EvaluationInputs};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{
    BudgetMeter, CapabilityId, EvalResult, EvaluationBudgets, EvaluationFailure,
};
use crate::eval::observe::{
    BuiltinInvocationObserver, ConsumedBuiltinInvocation, ExecutionEvent, ExecutionObserver,
};
use crate::eval::ports::{DependencyReadSink, ReadEvent};
use crate::eval::session::Session;
use crate::eval::values::{AreaKind, Value};
use crate::eval::vm::bytecode_vm::Vm;
use crate::eval::vm::expression_runner::ExpressionRunner;
use crate::eval::{compile, functions};
use crate::model::{CellAddress, Workbook};
use poirot_parsing::formula::UnaryOp;

#[test]
fn malformed_reference_encoding_returns_stale_plan() {
    for length in 0..4 {
        assert_eq!(
            OperandCursor(&[0; 4][..length]).word(),
            Err(EvaluationFailure::StalePlan)
        );
    }
    assert_eq!(
        OperandCursor(&[5, 0, 0, 0, b'a']).string(),
        Err(EvaluationFailure::StalePlan)
    );
    assert_eq!(
        OperandCursor(&[1, 0, 0, 0, 0xff]).string(),
        Err(EvaluationFailure::StalePlan)
    );
    assert!(OperandCursor(&[0; 16]).cell().is_err());
    assert!(decode_unary(u32::MAX).is_err());
    assert!(decode_binary(u32::MAX).is_err());
}

const AT: CellAddress = CellAddress { row: 3, col: 3 };

/// Records tree and bytecode execution, dependency, and builtin events for parity checks.
#[derive(Default)]
struct Events(Vec<String>);

impl ExecutionObserver for Events {
    fn observe(&mut self, event: ExecutionEvent<'_>) -> EvalResult<()> {
        self.0.push(format!("{event:?}"));
        Ok(())
    }
}

impl DependencyReadSink for Events {
    fn read(&mut self, event: ReadEvent) {
        self.0.push(format!("{event:?}"));
    }
}

impl BuiltinInvocationObserver for Events {
    fn consumed(&mut self, call: ConsumedBuiltinInvocation<'_>) -> EvalResult<()> {
        self.0.push(format!(
            "{:?} {:?} {} {} {}",
            call.target,
            call.handler,
            call.parsed_spelling,
            call.resolved_spelling,
            call.argument_count
        ));
        Ok(())
    }
}

fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    for (index, sheet) in ["One", "Two", "Three"].into_iter().enumerate() {
        workbook.add_sheet(sheet).unwrap();
        workbook
            .sheet_mut(index)
            .unwrap()
            .set_value("A1".parse().unwrap(), 2.)
            .unwrap();
        workbook
            .sheet_mut(index)
            .unwrap()
            .set_value("B1".parse().unwrap(), 4.)
            .unwrap();
    }
    workbook
}

fn snapshot(value: Value) -> String {
    match value {
        Value::Scalar(v) | Value::SelectedScalar(v, _) => format!("scalar {v:?}"),
        Value::Missing => "missing".into(),
        Value::Array(v) => format!("array {:?} {:?}", v.anchor, v.values),
        Value::Refs(v) | Value::SheetSpan(v) => format!(
            "refs {:?}",
            v.into_iter()
                .map(|a| (
                    matches!(a.kind, AreaKind::Cell),
                    a.sheet,
                    a.r0,
                    a.r1,
                    a.c0,
                    a.c1
                ))
                .collect::<Vec<_>>()
        ),
        Value::SignedRefs(v) => format!(
            "signed {:?}",
            v.into_iter()
                .map(|a| (
                    matches!(a.kind, AreaKind::Cell),
                    a.sheet,
                    a.r0,
                    a.r1,
                    a.c0,
                    a.c1
                ))
                .collect::<Vec<_>>()
        ),
    }
}

fn compare(
    syntax: &poirot_parsing::formula::Expr,
    budgets: EvaluationBudgets,
    registry: Option<&functions::extensions::ExtensionRegistry>,
) {
    {
        for builtin in [
            BuiltinSemanticsPolicy::PinnedPoi,
            BuiltinSemanticsPolicy::ExcelDocumentation,
        ] {
            compare_configured(
                syntax,
                budgets,
                registry,
                builtin,
                ArrayResultPolicy::ScalarIntersection,
            );
        }
    }
}

fn compare_configured(
    syntax: &poirot_parsing::formula::Expr,
    budgets: EvaluationBudgets,
    registry: Option<&functions::extensions::ExtensionRegistry>,
    builtin: BuiltinSemanticsPolicy,
    arrays: ArrayResultPolicy,
) {
    let workbook = workbook();
    let tree = compile::bound::bind_with_registry(syntax, registry);
    let program = compile::bytecode::compile_with_registry(syntax, registry).unwrap();
    let mut tree_events = Events::default();
    let mut vm_events = Events::default();
    let mut tree_reads = Events::default();
    let mut vm_reads = Events::default();
    let mut tree_calls = Events::default();
    let mut vm_calls = Events::default();
    let (tree_result, tree_usage, tree_extensions) = {
        let mut session = Session::new(
            &workbook,
            EvaluationInputs::default(),
            BudgetMeter::new(budgets).unwrap(),
        );
        session.state.runtime.budget.push_cell(EvaluationTarget {
            sheet: 0,
            address: AT,
        });
        session.sources.extensions = registry;

        session.policies.builtin_policy = builtin;
        session.policies.array_result_policy = arrays;
        session.ports.execution_observer = Some(&mut tree_events);
        session.ports.dependency_sink = Some(&mut tree_reads);
        session.ports.builtin_invocation_observer = Some(&mut tree_calls);
        let result = ExpressionRunner::new(&mut session)
            .expr(0, AT, &tree)
            .map(snapshot);
        (
            result,
            session.state.runtime.budget.usage.clone(),
            std::mem::take(&mut session.state.runtime.effects.extension_trace),
        )
    };
    let (vm_result, vm_usage, vm_extensions) = {
        let mut session = Session::new(
            &workbook,
            EvaluationInputs::default(),
            BudgetMeter::new(budgets).unwrap(),
        );
        session.state.runtime.budget.push_cell(EvaluationTarget {
            sheet: 0,
            address: AT,
        });
        session.sources.extensions = registry;

        session.policies.builtin_policy = builtin;
        session.policies.array_result_policy = arrays;
        session.ports.execution_observer = Some(&mut vm_events);
        session.ports.dependency_sink = Some(&mut vm_reads);
        session.ports.builtin_invocation_observer = Some(&mut vm_calls);
        let result = Vm::new(&mut session).run(0, AT, &program).map(snapshot);
        (
            result,
            session.state.runtime.budget.usage.clone(),
            std::mem::take(&mut session.state.runtime.effects.extension_trace),
        )
    };
    assert_eq!(tree_result, vm_result, "{syntax:?}");
    assert_eq!(tree_usage, vm_usage, "{syntax:?}");
    assert_eq!(tree_extensions, vm_extensions, "{syntax:?}");
    assert_eq!(tree_reads.0, vm_reads.0, "{syntax:?}");
    assert_eq!(tree_calls.0, vm_calls.0, "{syntax:?}");
    assert_eq!(tree_events.0, vm_events.0, "{syntax:?}");
}

#[test]
fn array_operator_tree_and_bytecode_match_values_reads_events_and_budgets() {
    for source in [
        "{1,2}+{10;20}",
        "-+{100,200}%",
        "{1,2;3,4}/{1;0}",
        "{1,2}<{2;1}",
        "{1,2}&{\"a\";\"b\"}",
        "{1,2}+{10,20,30}",
        "A1:B2+{10,20}",
        "{#N/A,#VALUE!}+A1:B1",
        "@({1,2}+1)",
        "IF(TRUE,{1,2},UNKNOWN())+1",
        "TRANSPOSE({1,2})*2",
        "SUMPRODUCT(A1:B2*{1,2})",
        "SUMPRODUCT(--((A1:B2&\"x\")=\"2x\"))",
        "SUM(A1:B2*{1,2})",
        "COLUMNS({1,2}+\"A\")",
        "SUMPRODUCT(IF(TRUE,A1:B2*2,UNKNOWN()))",
        "SUM(IF({TRUE,FALSE,TRUE},2,{30,5,70}))",
        "SUM(IF({TRUE;FALSE},{1,2,3},{10,20,30}))",
        "SUM(IF({TRUE,TRUE},RAND(),UNKNOWN()))",
        "SUMPRODUCT(IF(A1:B2>1,A1:B2,0))",
        "ISREF(IF({TRUE},A1:B2,0))",
        "SUM(IFS({TRUE,FALSE,TRUE},{3,100,7},TRUE,5))",
        "SUM(IFS({TRUE;FALSE},{1,2,3},TRUE,9))",
        "SUMPRODUCT(IFS(A1:B2>1,A1:B2,TRUE,0))",
        "ISREF(IFS(TRUE,A1:B2,TRUE,0))",
        "ISREF(IFS({TRUE},A1:B2,TRUE,0))",
        "SUM(CHOOSE({1,2,1},{3,100,7},5))",
        "SUMPRODUCT(CHOOSE({1;2},A1:B2,0))",
        "SUM(CHOOSE({1.9,2.9},2,3))",
        "ISREF(CHOOSE(1,A1:B2,0))",
        "ISREF(CHOOSE({1},A1:B2,0))",
        "SUM(SWITCH({1,2,1},1,{3,100,7},2,5,11))",
        "SUM(SWITCH(1,{1,2},2,3))",
        "SUMPRODUCT(SWITCH(A1:B2>1,TRUE,A1:B2,FALSE,0))",
        "ISREF(SWITCH(1,1,A1:B2,0))",
        "ISREF(SWITCH({1},1,A1:B2,0))",
        "SUMPRODUCT(--ISNUMBER(A1:B2),{1,2;4,8})",
        "SUMPRODUCT(--ISTEXT({1,\"2\";TRUE,#N/A}),{1,2;4,8})",
        "ISNUMBER(ISNUMBER(A1:B2))",
        "ISLOGICAL({TRUE})",
        "ISBLANK(IFS(TRUE,))",
    ] {
        let syntax = poirot_parsing::formula::parse_formula(source).unwrap();
        {
            for builtin in [
                BuiltinSemanticsPolicy::PinnedPoi,
                BuiltinSemanticsPolicy::ExcelDocumentation,
            ] {
                for budgets in [
                    EvaluationBudgets::default(),
                    EvaluationBudgets {
                        materialized_values: 3,
                        ..Default::default()
                    },
                    EvaluationBudgets {
                        run_work: 10,
                        ..Default::default()
                    },
                ] {
                    for arrays in [
                        ArrayResultPolicy::ScalarIntersection,
                        ArrayResultPolicy::DynamicSpill,
                    ] {
                        compare_configured(&syntax, budgets, None, builtin, arrays);
                    }
                }
            }
        }
    }
}

fn source(source: &str, budgets: EvaluationBudgets) {
    compare(
        &poirot_parsing::formula::parse_formula(source)
            .unwrap_or_else(|error| panic!("{source}: {error}")),
        budgets,
        None,
    );
}

fn compile(source: &str) -> Compiled {
    compile::bytecode::compile_with_registry(
        &poirot_parsing::formula::parse_formula(source).unwrap(),
        None,
    )
    .unwrap()
}

#[test]
fn bytecode_stack_reuses_capacity_across_nested_cells_and_lazy_arguments() {
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(
            "A2".parse().unwrap(),
            "10+IF(TRUE,SUM({1,2;3,4}),UNKNOWN())",
        )
        .unwrap();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let program =
        compile("100+IF(TRUE,A2+SUM(One:Three!A1)+SUM({5,6})+SUMPRODUCT({2},{3}),UNKNOWN())");
    assert_eq!(
        snapshot(Vm::new(&mut session).run(0, AT, &program).unwrap()),
        "scalar Number(143.0)"
    );
    let buffers = |session: &Session<'_>| {
        let vm = &session.state.runtime.vm;
        assert!(vm.values.is_empty());
        assert!(vm.operands.is_empty());
        [
            (vm.values.as_ptr() as usize, vm.values.capacity()),
            (vm.operands.as_ptr() as usize, vm.operands.capacity()),
        ]
    };
    let allocations = buffers(&session);
    assert!(allocations.iter().all(|(_, capacity)| *capacity > 0));
    for _ in 0..3 {
        session.state.runtime.memo.cache.clear();
        assert_eq!(
            snapshot(Vm::new(&mut session).run(0, AT, &program).unwrap()),
            "scalar Number(143.0)"
        );
        assert_eq!(buffers(&session), allocations);
    }
    let fresh = Session::with_context(&workbook, EvaluationInputs::default());
    assert_eq!(fresh.state.runtime.vm.values.capacity(), 0);
    assert_eq!(fresh.state.runtime.vm.operands.capacity(), 0);
}

#[test]
fn lazy_argument_frames_grow_only_through_forced_indices() {
    let workbook = workbook();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    // IFS selects the first true branch; the other branches must not draw RAND.
    // https://support.microsoft.com/en-us/office/ifs-function-36329a26-37b2-467c-972b-4a39bd951d45
    let suffix = ",FALSE,RAND()".repeat(63);
    let source = format!("100+IFS(TRUE,SUMPRODUCT({{2}},{{3}}){suffix})");
    assert_eq!(
        snapshot(Vm::new(&mut session).run(0, AT, &compile(&source)).unwrap()),
        "scalar Number(106.0)"
    );
    assert!(session.state.runtime.vm.is_idle());
    assert!(
        session.state.runtime.vm.operands.capacity() <= 8,
        "unselected arguments must not reserve slots"
    );
    assert!(
        session
            .state
            .runtime
            .budget
            .usage
            .random_accesses
            .is_empty()
    );

    // A late selection legitimately needs a wider frame. Nested calls still
    // preserve caller values, and false result branches remain unevaluated.
    let prefix = "FALSE,RAND(),".repeat(63);
    let source = format!("100+IFS({prefix}TRUE,SUMPRODUCT({{2}},{{3}}))");
    assert_eq!(
        snapshot(Vm::new(&mut session).run(0, AT, &compile(&source)).unwrap()),
        "scalar Number(106.0)"
    );
    assert!(session.state.runtime.vm.is_idle());
    assert!(
        session
            .state
            .runtime
            .budget
            .usage
            .random_accesses
            .is_empty()
    );
}

#[test]
fn bytecode_stack_returns_to_frame_base_after_budget_failure() {
    let workbook = workbook();
    // Abort after partial operands, including nested lazy arguments and array construction.
    for source in [
        "100+(1+2)",
        "100+IF(TRUE,1+2,UNKNOWN())",
        "100+SUM({1,2;3,4})",
    ] {
        let program = compile(source);
        let limits = (1..25)
            .map(|run_work| EvaluationBudgets {
                run_work,
                ..EvaluationBudgets::default()
            })
            .chain((1..8).map(|depth| EvaluationBudgets {
                depth,
                ..EvaluationBudgets::default()
            }));
        let mut failures = 0;
        for limits in limits {
            let mut session = Session::new(
                &workbook,
                EvaluationInputs::default(),
                BudgetMeter::new(limits).unwrap(),
            );
            failures += usize::from(Vm::new(&mut session).run(0, AT, &program).is_err());
            assert!(session.state.runtime.vm.is_idle(), "{source}, {limits:?}");
            session.state.runtime.budget = BudgetMeter::new(EvaluationBudgets::default()).unwrap();
            assert_eq!(
                snapshot(Vm::new(&mut session).run(0, AT, &compile("17")).unwrap()),
                "scalar Number(17.0)"
            );
            assert!(session.state.runtime.vm.is_idle());
        }
        assert!(failures > 0);
    }
}

#[test]
fn bytecode_stack_unwinds_nested_observer_refusals_with_tree_trace_parity() {
    /// Rejects a chosen event ordinal to test nested VM frame unwinding.
    struct Reject {
        events: Vec<String>,
        ordinal: usize,
    }
    impl ExecutionObserver for Reject {
        fn observe(&mut self, event: ExecutionEvent<'_>) -> EvalResult<()> {
            self.events.push(format!("{event:?}"));
            if self.events.len() == self.ordinal {
                Err(EvaluationFailure::StalePlan)
            } else {
                Ok(())
            }
        }
    }
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(
            "A2".parse().unwrap(),
            "10+IF(TRUE,SUM({1,2;3,4}),UNKNOWN())",
        )
        .unwrap();
    let syntax =
        poirot_parsing::formula::parse_formula("100+IF(TRUE,A2+SUM({5,6}),UNKNOWN())").unwrap();
    let tree = compile::bound::bind_with_registry(&syntax, None);
    let program = compile::bytecode::compile_with_registry(&syntax, None).unwrap();
    let probe = compile("17");
    // Refuse every event in turn, covering Begin, successful Complete and ancestor unwind.
    let run = |vm: bool, ordinal| {
        let mut observer = Reject {
            events: Vec::new(),
            ordinal,
        };
        let (result, usage) = {
            let mut session = Session::with_context(&workbook, EvaluationInputs::default());
            session.ports.execution_observer = Some(&mut observer);
            let result = if vm {
                Vm::new(&mut session).run(0, AT, &program)
            } else {
                ExpressionRunner::new(&mut session).expr(0, AT, &tree)
            }
            .map(snapshot);
            assert!(session.state.runtime.vm.is_idle());
            let usage = session.state.runtime.budget.usage.clone();
            assert_eq!(
                snapshot(Vm::new(&mut session).run(0, AT, &probe).unwrap()),
                "scalar Number(17.0)"
            );
            assert!(session.state.runtime.vm.is_idle());
            (result, usage)
        };
        (result, usage, observer.events)
    };
    // The final literal probe emits its own Begin/Complete pair; refuse only
    // main-expression events, then verify the observed probe still succeeds.
    let events = run(true, usize::MAX).2.len() - 2;
    for ordinal in 1..=events {
        assert_eq!(
            run(false, ordinal),
            run(true, ordinal),
            "observer event {ordinal}"
        );
    }
}

#[test]
fn bytecode_vm_matches_tree_families_references_and_lazy_errors() {
    for formula in [
        "1+2*3^2",
        "-0%",
        "+\"hello\"",
        "1/0",
        "\"A\"=\"a\"",
        "\"a\"&2",
        "@A1",
        "{1,2;3,4}",
        "SUM(A1:B1)",
        "SUM(One:Three!A1)",
        "SUM(One:Three!A1:B1)",
        "A1:B1",
        "(A1,B1)",
        "A1:B2 B1:C2",
        "SUM(A:A)",
        "SUM(1:1)",
        "One!missing",
        "missing",
        "IF(TRUE,7,UNKNOWN())",
        "IF(FALSE,1/0,9)",
        "IF(TRUE,,1/0)",
        "IFERROR(1/0,IFNA(#N/A,8))",
        "CHOOSE(2,1/0,9)",
        "IFERROR(A1,UNKNOWN())",
        "UNKNOWN(1/0)",
        "IF(1/0)",
        "DATE(2020,2,3)",
        "AND(TRUE,FALSE)",
        "AVERAGE({1,2;3,4})",
        "ROW(A1)",
        "INDEX(A1:B1,1,2)",
        "MATCH(2,{1,2,3},0)",
        "ISNUMBER(2)",
        "ABS(-3)",
        "LEFT(\"hello\",2)",
        "PV(0.1,2,3)",
        "BIN2DEC(10)",
        "MMULT({1,2},{3;4})",
        "SUMIF(A1:B1,\">2\")",
        "DSUM({\"a\",\"b\";1,2},\"b\",{\"a\";1})",
        "STDEV.S(1,2)",
        "QUARTILE.INC({1,2,3},1)",
        "RANK.EQ(2,{1,2,3})",
        "PERCENTILE.INC({1,2,3},0.5)",
        "_xlfn.IFNA(#N/A,2)",
        "RAND()",
        "RANDBETWEEN(1,4)",
    ] {
        source(formula, EvaluationBudgets::default());
    }
}

#[test]
fn bytecode_vm_matches_tree_budget_admission_order() {
    for formula in [
        "{1,2;3,4}",
        "SUM(A1:B1)+IF(FALSE,UNKNOWN(),5)",
        "IFERROR(1/0,2)",
        "A1+B1",
    ] {
        for work in 1..65 {
            source(
                formula,
                EvaluationBudgets {
                    run_work: work,
                    ..EvaluationBudgets::default()
                },
            );
        }
        for work in 1..65 {
            source(
                formula,
                EvaluationBudgets {
                    cell_work: work,
                    ..EvaluationBudgets::default()
                },
            );
        }
        for allocation in 1..7 {
            source(
                formula,
                EvaluationBudgets {
                    materialized_values: allocation,
                    ..EvaluationBudgets::default()
                },
            );
        }
        for depth in 1..6 {
            source(
                formula,
                EvaluationBudgets {
                    depth,
                    ..EvaluationBudgets::default()
                },
            );
        }
    }
}

#[test]
fn bytecode_vm_preserves_missing_array_provenance_and_depth_truncation() {
    compare(
        &poirot_parsing::formula::Expr::Function {
            name: "INDEX".into(),
            args: vec![
                poirot_parsing::formula::Expr::Array(vec![vec![
                    poirot_parsing::formula::Expr::Number(1.),
                    poirot_parsing::formula::Expr::Missing,
                ]]),
                poirot_parsing::formula::Expr::Number(1.),
                poirot_parsing::formula::Expr::Number(2.),
            ],
        },
        EvaluationBudgets::default(),
        None,
    );
    let array_syntax = poirot_parsing::formula::Expr::Array(vec![vec![
        poirot_parsing::formula::parse_formula("A1").unwrap(),
        poirot_parsing::formula::parse_formula("IFERROR(1/0,2)").unwrap(),
    ]]);
    for work in 1..32 {
        compare(
            &array_syntax,
            EvaluationBudgets {
                run_work: work,
                ..EvaluationBudgets::default()
            },
            None,
        );
    }
    let mut syntax = poirot_parsing::formula::Expr::Number(1.);
    for _ in 0..132 {
        syntax = poirot_parsing::formula::Expr::Unary {
            op: UnaryOp::Plus,
            operand: Box::new(syntax),
        };
    }
    compare(&syntax, EvaluationBudgets::default(), None);
}

#[test]
fn bytecode_vm_matches_extension_binding_laziness_and_namespace_spelling() {
    use functions::extensions::*;
    for evaluation in [
        ExtensionEvaluationStrategy::Deferred,
        ExtensionEvaluationStrategy::Eager,
    ] {
        let mut registry = ExtensionRegistry::new();
        registry
            .register(
                ExtensionNamespace::UserDefined,
                "CUSTOM",
                "v1",
                ExtensionContract {
                    arity: functions::registry::Arity {
                        minimum: 1,
                        maximum: 2,
                    },
                    evaluation,
                    capabilities: ExtensionCapabilities {
                        references: true,
                        arrays: true,
                        context: false,
                    },
                    cacheability: ExtensionCacheability::Deterministic,
                },
                |_: &mut dyn ExtensionHost| Ok(ExtensionOutput::Argument(0)),
            )
            .unwrap();
        registry
            .set_external_namespace(Some(
                functions::external_namespace::ExternalFunctionNamespace::from_stored_names(
                    ["CuStOm", "sUm"],
                    4,
                )
                .unwrap(),
            ))
            .unwrap();
        for source in [
            "CUSTOM(A1,UNKNOWN())",
            "CUSTOM()",
            "CUSTOM(1,2,UNKNOWN())",
            "CUSTOM({1,2;3,4})",
            "IF(FALSE,CUSTOM(UNKNOWN()),7)",
            "SUM(A1:B1)",
        ] {
            compare(
                &poirot_parsing::formula::parse_formula(source).unwrap(),
                EvaluationBudgets::default(),
                Some(&registry),
            );
        }
        let syntax = poirot_parsing::formula::Expr::Function {
            name: "unsupported_Ω".into(),
            args: vec![poirot_parsing::formula::Expr::Number(1.)],
        };
        compare(&syntax, EvaluationBudgets::default(), Some(&registry));
    }
}

#[test]
fn bytecode_vm_unwinds_observer_rejections_before_next_expression() {
    /// Rejects after a controlled number of accepted events to test session recovery.
    struct Rejecting {
        remaining: usize,
        events: Vec<String>,
    }
    impl ExecutionObserver for Rejecting {
        fn observe(&mut self, event: ExecutionEvent<'_>) -> EvalResult<()> {
            self.events.push(format!("{event:?}"));
            if self.remaining == 0 {
                Err(EvaluationFailure::ResourceLimit {
                    capability: CapabilityId::FunctionAdapter,
                    source: None,
                    detail: "test observer rejected event".into(),
                })
            } else {
                self.remaining -= 1;
                Ok(())
            }
        }
    }
    let workbook = workbook();
    let syntax = poirot_parsing::formula::parse_formula("IFERROR(1/0,2)+3").unwrap();
    let tree = compile::bound::bind(&syntax);
    let program = compile::bytecode::compile(&syntax).unwrap();
    for limit in 0..30 {
        let run = |bytecode| {
            let mut observer = Rejecting {
                remaining: limit,
                events: Vec::new(),
            };
            let (result, usage) = {
                let mut session = Session::with_context(&workbook, EvaluationInputs::default());
                session.ports.execution_observer = Some(&mut observer);
                let result = if bytecode {
                    Vm::new(&mut session).run(0, AT, &program)
                } else {
                    ExpressionRunner::new(&mut session).expr(0, AT, &tree)
                }
                .map(snapshot);
                session.ports.execution_observer = None;
                assert_eq!(
                    ExpressionRunner::new(&mut session)
                        .expr(0, AT, &Expression::Number(17.))
                        .map(snapshot),
                    Ok("scalar Number(17.0)".into())
                );
                (result, session.state.runtime.budget.usage.clone())
            };
            (result, usage, observer.events)
        };
        assert_eq!(run(false), run(true), "observer limit {limit}");
    }
}

#[test]
fn bytecode_vm_without_execution_observer_preserves_budget_and_unwind_parity() {
    let workbook = workbook();
    for source in [
        "1+2*3",
        "SUM(A1:B1)+IFERROR(1/0,2)",
        "IF(FALSE,UNKNOWN(),IF(TRUE,7,1/0))",
        "{1,2;3,4}",
    ] {
        let syntax = poirot_parsing::formula::parse_formula(source).unwrap();
        let tree = compile::bound::bind(&syntax);
        let program = compile::bytecode::compile(&syntax).unwrap();
        let budgets = std::iter::once(EvaluationBudgets::default())
            .chain((1..65).map(|run_work| EvaluationBudgets {
                run_work,
                ..EvaluationBudgets::default()
            }))
            .chain((1..7).map(|depth| EvaluationBudgets {
                depth,
                ..EvaluationBudgets::default()
            }))
            .chain((1..7).map(|materialized_values| EvaluationBudgets {
                materialized_values,
                ..EvaluationBudgets::default()
            }));
        for limits in budgets {
            let run = |bytecode| {
                let mut session = Session::new(
                    &workbook,
                    EvaluationInputs::default(),
                    BudgetMeter::new(limits).unwrap(),
                );
                session.state.runtime.budget.push_cell(EvaluationTarget {
                    sheet: 0,
                    address: AT,
                });
                assert!(session.ports.execution_observer.is_none());
                let result = if bytecode {
                    Vm::new(&mut session).run(0, AT, &program)
                } else {
                    ExpressionRunner::new(&mut session).expr(0, AT, &tree)
                }
                .map(snapshot);
                let usage = session.state.runtime.budget.usage.clone();
                // A depth failure must unwind every admitted frame before the
                // next expression. Exhausted work remains exhausted in both paths.
                let next = ExpressionRunner::new(&mut session)
                    .expr(0, AT, &Expression::Number(17.))
                    .map(snapshot);
                (
                    result,
                    usage,
                    next,
                    session.state.runtime.budget.usage.clone(),
                )
            };
            assert_eq!(run(false), run(true), "{source}, {limits:?}");
        }
    }
}
