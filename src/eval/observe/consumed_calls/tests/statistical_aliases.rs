use super::support::{A1, B1, Bounded, Entry, names, plan, workbook};
use crate::eval::config::EvaluationInputs;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::functions::registry::FunctionId;
use crate::eval::limits::EvaluationFailure;
use crate::eval::session::{FormulaExecution, Session};
use crate::model::CellValue;

#[test]
fn statistical_aliases_consume_the_actual_typed_handler_and_keep_spellings() {
    use crate::eval::functions::registry::{FunctionId, StatisticsFunction};
    let workbook = workbook();
    for (name, handler, expected) in [
        ("STDEV.S", StatisticsFunction::Stdev, 1.),
        ("STDEV", StatisticsFunction::Stdev, 1.),
        ("STDEVP", StatisticsFunction::StdevP, (2_f64 / 3.).sqrt()),
        ("STDEV.P", StatisticsFunction::StdevP, (2_f64 / 3.).sqrt()),
        ("VAR.S", StatisticsFunction::Var, 1.),
        ("VAR", StatisticsFunction::Var, 1.),
        ("VARP", StatisticsFunction::VarP, 2. / 3.),
        ("VAR.P", StatisticsFunction::VarP, 2. / 3.),
    ] {
        for prefix in ["", "_xlfn."] {
            if !prefix.is_empty() && !name.contains('.') {
                continue;
            }
            let spelling = format!("{prefix}{name}");
            let mut observer = Bounded::new(20, 4096);
            let mut session = Session::with_context(&workbook, EvaluationInputs::default());
            session.ports.builtin_invocation_observer = Some(&mut observer);
            let selected = plan(&format!("{spelling}({{1,2,3}})"));
            let target = EvaluationTarget {
                sheet: 0,
                address: A1,
            };
            let value = FormulaExecution::new(&mut session)
                .bound_at(target, &selected)
                .unwrap();
            drop(session);
            let CellValue::Number(value) = value else {
                panic!("{spelling}")
            };
            assert!((value - expected).abs() < 1e-12, "{spelling}");
            assert_eq!(
                observer.entries,
                vec![Entry {
                    target,
                    handler: FunctionId::Statistics(handler),
                    parsed: spelling.clone(),
                    resolved: spelling,
                    arity: 1,
                }]
            );
        }
    }
}

#[test]
fn legacy_statistical_names_with_atp_prefix_do_not_enter_a_builtin() {
    let workbook = workbook();
    for name in ["STDEV", "STDEVP", "VAR", "VARP"] {
        let mut observer = Bounded::new(20, 4096);
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.ports.builtin_invocation_observer = Some(&mut observer);
        let selected = plan(&format!("_xlfn.{name}({{1,2,3}})"));
        let target = EvaluationTarget {
            sheet: 0,
            address: A1,
        };
        let value = FormulaExecution::new(&mut session)
            .bound_at(target, &selected)
            .unwrap();
        drop(session);
        assert_eq!(
            value,
            CellValue::Error(crate::model::CellError::Name),
            "{name}"
        );
        assert!(observer.entries.is_empty(), "{name}");
    }
}

#[test]
fn documented_excel_alias_observations_are_typed_and_lazy() {
    use crate::eval::functions::registry::StatisticsFunction;
    let mut workbook = workbook();
    workbook.sheet_mut(0).unwrap().set_value(A1, 4.).unwrap();
    for (formula, handler, spelling) in [
        (
            "PERCENTILE.INC({1,3},0.5)",
            StatisticsFunction::Percentile,
            "PERCENTILE.INC",
        ),
        (
            "_xlfn.QUARTILE.INC({1,3},2)",
            StatisticsFunction::QuartileInc,
            "_xlfn.QUARTILE.INC",
        ),
        ("RANK.EQ(4,A1:A1,-1)", StatisticsFunction::RankEq, "RANK.EQ"),
        (
            "_xlfn.RANK.EQ(4,{1,4,4},-0.1)",
            StatisticsFunction::RankEq,
            "_xlfn.RANK.EQ",
        ),
    ] {
        let mut observer = Bounded::new(10, 4096);
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.policies.builtin_policy =
            crate::eval::config::BuiltinSemanticsPolicy::ExcelDocumentation;
        session.ports.builtin_invocation_observer = Some(&mut observer);
        FormulaExecution::new(&mut session)
            .bound_at(
                EvaluationTarget {
                    sheet: 0,
                    address: B1,
                },
                &plan(formula),
            )
            .unwrap();
        drop(session);
        assert_eq!(observer.entries.len(), 1);
        assert_eq!(observer.entries[0].handler, FunctionId::Statistics(handler));
        assert_eq!(observer.entries[0].parsed, spelling);
        assert_eq!(observer.entries[0].resolved, spelling);
        assert_eq!(
            observer.entries[0].arity,
            if handler == StatisticsFunction::RankEq {
                3
            } else {
                2
            }
        );
    }
    let mut observer = Bounded::new(10, 4096);
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.policies.builtin_policy =
        crate::eval::config::BuiltinSemanticsPolicy::ExcelDocumentation;
    session.ports.builtin_invocation_observer = Some(&mut observer);
    FormulaExecution::new(&mut session)
        .bound_at(
            EvaluationTarget {
                sheet: 0,
                address: B1,
            },
            &plan("IF(FALSE,RANK.EQ(4,A1:A1,-1),7)"),
        )
        .unwrap();
    drop(session);
    assert_eq!(names(&observer), ["IF"]);
}

#[test]
fn documented_alias_execution_observation_and_dependency_reads_match_handler() {
    use crate::eval::observe::execution::{ExecutionEvent, ExecutionObserver, OperationKind};
    #[derive(Default)]
    /// Records handlers actually entered through documented statistical aliases.
    struct Events(Vec<FunctionId>);
    impl ExecutionObserver for Events {
        fn observe(&mut self, event: ExecutionEvent<'_>) -> Result<(), EvaluationFailure> {
            if let ExecutionEvent::Begin(operation) = event
                && let OperationKind::Expression {
                    handler: Some(handler),
                } = operation.kind
            {
                self.0.push(handler);
            }
            Ok(())
        }
    }
    #[derive(Default)]
    /// Collects policy and context reads produced by alias execution.
    struct Reads(Vec<crate::eval::ports::ReadEvent>);
    impl crate::eval::ports::DependencyReadSink for Reads {
        fn read(&mut self, event: crate::eval::ports::ReadEvent) {
            self.0.push(event);
        }
    }
    let workbook = workbook();
    for (formula, expected, read) in [
        (
            "RANK.EQ(4,{1,4,4},-0.1)",
            vec![FunctionId::Statistics(
                crate::eval::functions::registry::StatisticsFunction::RankEq,
            )],
            true,
        ),
        (
            "QUARTILE.INC({1,3},2)",
            vec![FunctionId::Statistics(
                crate::eval::functions::registry::StatisticsFunction::QuartileInc,
            )],
            true,
        ),
        (
            "IF(FALSE,QUARTILE.INC({1,3},2),7)",
            vec![FunctionId::Logical(
                crate::eval::functions::registry::LogicalFunction::If,
            )],
            // The alias stays skipped, but IF itself selects profile-dependent array semantics.
            true,
        ),
    ] {
        let mut events = Events::default();
        let mut reads = Reads::default();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.policies.builtin_policy =
            crate::eval::config::BuiltinSemanticsPolicy::ExcelDocumentation;
        session.ports.execution_observer = Some(&mut events);
        session.ports.dependency_sink = Some(&mut reads);
        FormulaExecution::new(&mut session)
            .bound_at(
                EvaluationTarget {
                    sheet: 0,
                    address: B1,
                },
                &plan(formula),
            )
            .unwrap();
        drop(session);
        assert_eq!(events.0, expected);
        assert_eq!(
            reads
                .0
                .iter()
                .any(|r| r.read == crate::eval::ports::DependencyRead::BuiltinPolicy),
            read
        );
    }
}
