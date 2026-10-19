use super::*;
const AT: CellAddress = CellAddress { row: 0, col: 0 };
fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
}
fn expression(formula: &str) -> Expr {
    bound::bind(&poirot_parsing::formula::parse_formula(formula).unwrap())
}
#[test]
fn demand_memoization_preserves_references_without_reading_cells() {
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(AT, "1/0", Number(99.))
        .unwrap();
    let arguments = [expression("A1"), expression("UNKNOWN_FUNCTION()")];
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    assert_eq!(host.kind(0).unwrap(), OperandKind::Reference);
    let expression_count = host.session.budget.usage.expressions;
    assert_eq!(host.kind(0).unwrap(), OperandKind::Reference);
    assert_eq!(host.session.budget.usage.expressions, expression_count);
    assert_eq!(host.session.budget.usage.cell_reads, 0);
    assert_eq!(host.memo.len(), 1);
    assert!(matches!(
        host.scalar(0, MissingPolicy::Reject),
        Err(FunctionFailure::Spreadsheet(CellError::Div0))
    ));
    assert_eq!(host.memo.len(), 1);
}
#[test]
fn missing_and_blank_and_array_missing_are_distinct() {
    let workbook = workbook();
    let arguments = [
        Expr::Missing,
        expression("A1"),
        Expr::Array(vec![
            vec![Expr::Number(1.), Expr::Missing],
            vec![Expr::Missing, Expr::Number(2.)],
        ]),
    ];
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    assert_eq!(host.kind(0).unwrap(), OperandKind::Missing);
    assert!(matches!(
        host.scalar(0, MissingPolicy::Reject),
        Err(FunctionFailure::Spreadsheet(CellError::Value))
    ));
    assert_eq!(host.scalar(0, MissingPolicy::Zero).unwrap(), Number(0.));
    assert_eq!(host.scalar(0, MissingPolicy::Blank).unwrap(), Blank);
    let blank = host.scalar_data(1, MissingPolicy::Reject).unwrap();
    assert_eq!(blank.value, Blank);
    assert!(!blank.missing);
    assert_eq!(blank.origin, OperandOrigin::ReferencedCell);
    assert_eq!(
        host.argument(2).unwrap().array_missing(),
        &[vec![false, true], vec![true, false]]
    );
    let last = host.collection(2, TraversalPolicy::LastAreaRow).unwrap();
    assert_eq!(last.len(), 2);
    assert!(last[0].missing);
    assert!(!last[1].missing);
    assert_eq!(last[1].value, Number(2.));
    assert_eq!(last[1].origin, OperandOrigin::ArrayElement);
}
#[test]
fn union_is_not_a_dense_rectangle_and_sparse_traversal_retains_origin() {
    let mut workbook = workbook();
    workbook.sheet_mut(0).unwrap().set_value(AT, 3.).unwrap();
    let arguments = [expression("(A1,B2)")];
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    assert!(matches!(
        host.collection(0, TraversalPolicy::DenseRectangle),
        Err(FunctionFailure::Spreadsheet(CellError::Value))
    ));
    let sparse = host
        .collection(0, TraversalPolicy::SparseAggregation)
        .unwrap();
    assert_eq!(sparse.len(), 1);
    assert_eq!(sparse[0].value, Number(3.));
    assert_eq!(sparse[0].origin, OperandOrigin::ReferencedCell);
    assert_eq!(host.argument(0).unwrap().kind(), OperandKind::Reference);
}
#[test]
fn context_is_captured_and_helper_estimates_abort_before_execution() {
    let workbook = workbook();
    let context = EvaluationInputs {
        as_of_time: UNIX_EPOCH,
        randomness_seed: 17,
        ..EvaluationInputs::default()
    };
    let mut session = Session::new(
        &workbook,
        context,
        BudgetMeter::new(EvaluationBudgets {
            run_work: 2,
            ..EvaluationBudgets::default()
        })
        .unwrap(),
    );
    let mut host = CallHost::new(&mut session, 0, AT, &[]);
    assert_eq!(host.context().randomness_seed, 17);
    assert_eq!(host.context().as_of_time, UNIX_EPOCH);
    assert_eq!(host.date_system(), workbook.date_system);
    assert_eq!(host.target().address, AT);
    assert_eq!(host.remaining_work(), 2);
    host.charge_work(2).unwrap();
    assert_eq!(host.remaining_work(), 0);
    assert!(matches!(
        host.charge_work(1),
        Err(FunctionFailure::Operational(EvaluationFailure::Budget(
            BudgetFailure {
                kind: BudgetKind::RunWork,
                ..
            }
        )))
    ));
    assert_eq!(host.session.budget.usage.helper_operations, 2);
}
#[test]
fn nested_selected_missing_is_not_a_blank_reference() {
    let workbook = workbook();
    let arguments = [expression("IFS(TRUE,)"), expression("A1"), Expr::Missing];
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    for index in [0, 2] {
        assert_eq!(host.kind(index).unwrap(), OperandKind::Missing);
        assert!(
            host.scalar_data(index, MissingPolicy::Blank)
                .unwrap()
                .missing
        );
        assert!(host.read_cell(index, 0, 0).unwrap().missing);
        assert!(matches!(
            host.argument(index).unwrap().into_value(),
            Value::Missing
        ));
    }
    assert_eq!(host.kind(1).unwrap(), OperandKind::Reference);
    let blank = host.scalar_data(1, MissingPolicy::Blank).unwrap();
    assert_eq!(blank.value, Blank);
    assert!(!blank.missing);
}
#[test]
fn indexed_reads_preserve_reference_kind_and_resize_only_references() {
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(CellAddress { row: 1, col: 0 }, Number(42.))
        .unwrap();
    let arguments = [expression("A1"), expression("A1:A1"), expression("{1}")];
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    assert!(!host.describe(0).unwrap().is_area);
    assert!(host.describe(1).unwrap().is_area);
    assert!(host.describe(2).unwrap().is_area);
    assert_eq!(host.session.budget.usage.cell_reads, 0);
    assert!(matches!(
        host.read_cell(1, 1, 0),
        Err(FunctionFailure::Spreadsheet(Ref))
    ));
    assert_eq!(host.read_relative_cell(1, 1, 0).unwrap().value, Number(42.));
    assert_eq!(host.session.budget.usage.cell_reads, 1);
    assert!(matches!(
        host.read_relative_cell(2, 1, 0),
        Err(FunctionFailure::Spreadsheet(Ref))
    ));
    assert!(matches!(
        host.read_relative_cell(1, MAX_ROWS as usize, 0),
        Err(FunctionFailure::Spreadsheet(Ref))
    ));
}
#[test]
fn pinned_numeric_conversion_preserves_inputs_and_source_precision() {
    let workbook = workbook();
    let arguments = [expression("2.9999999999999996"), expression(r#"" 1e2 ""#)];
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    let n = host
        .coercev2_argument_number(0, MissingPolicy::Reject, NumericPolicy::PinnedPoi)
        .unwrap();
    assert_eq!(
        n.to_bits(),
        2.9999999999999996f64.to_bits(),
        "bound and scalar host preserve literal bits"
    );
    assert_eq!(n.trunc(), 2.);
    assert_eq!(
        poirot_coercionv2::compatibility::coercev2_number_to_text(n),
        "3",
        "same dependency and exact host input"
    );
    assert_eq!(
        host.coercev2_number_to_text(n, NumericPolicy::PinnedPoi)
            .unwrap(),
        "3"
    );
    assert_eq!(
        host.coercev2_argument_number(1, MissingPolicy::Reject, NumericPolicy::PinnedPoi)
            .unwrap(),
        100.
    );
    assert!(
        host.equal(&Number(n), &Number(3.), NumericPolicy::PinnedPoi)
            .unwrap()
    );
}
#[test]
fn numeric_datetime_host_preserves_results_failures_and_actual_effects() {
    let workbook = workbook();
    let context = EvaluationInputs {
        as_of_time: UNIX_EPOCH + Duration::from_secs(300_000_000_000),
        randomness_seed: 0,
        localization: Localization {
            utc_offset_seconds: 0,
            date_order: DateOrder::MonthDayYear,
        },
    };
    let mut session = Session::with_context(&workbook, context);
    {
        let mut host = CallHost::new(&mut session, 0, AT, &[]);
        assert_eq!(
            host.coercev2_number_text("1 Jan 2024", NumericPolicy::PinnedPoi)
                .unwrap(),
            45292.0
        );
        assert_eq!(
            host.coercev2_number_text("0:30 PM", NumericPolicy::PinnedPoi)
                .unwrap(),
            45000.0 / 86400.0
        );
        assert!(matches!(
            host.coercev2_number_text("garbage", NumericPolicy::PinnedPoi),
            Err(FunctionFailure::Spreadsheet(CellError::Value))
        ));
        assert!(!host.session.numeric_date_effects[&(0, AT)].default_year);
        assert!(matches!(
            host.coercev2_number_text("1 Jan", NumericPolicy::PinnedPoi),
            Err(FunctionFailure::Operational(
                EvaluationFailure::UnsupportedCapability {
                    capability: CapabilityId::DateParsing,
                    ..
                }
            ))
        ));
    }
    assert_eq!(
        session.numeric_date_effects[&(0, AT)],
        poirot_coercionv2::date_time::Effects {
            default_year: true,
            locale: true,
            timezone: true
        }
    );
    let mut session = Session::with_context(
        &workbook,
        EvaluationInputs {
            as_of_time: UNIX_EPOCH + Duration::from_secs(1704067200),
            ..context
        },
    );
    let mut host = CallHost::new(&mut session, 0, AT, &[]);
    assert_eq!(
        host.coercev2_number_text("1 Jan", NumericPolicy::PinnedPoi)
            .unwrap(),
        45292.0
    );
}
#[test]
fn numeric_datetime_actual_math_bridge_and_iferror_boundary() {
    let mut workbook = workbook();
    // Fallback remains source-1900 even when the workbook is date1904.
    workbook.date_system = poirot_model::DateSystem::Excel1904;
    let context = EvaluationInputs {
        as_of_time: UNIX_EPOCH + Duration::from_secs(300_000_000_000),
        randomness_seed: 0,
        localization: Localization {
            utc_offset_seconds: 0,
            date_order: DateOrder::MonthDayYear,
        },
    };
    for (formula, expected) in [
        (r#"ABS("1 Jan 2024")"#, 45292.0),
        (r#"ABS("1 Jan +10000")"#, 2958466.0),
        (r#"ABS("32 12:00")"#, 0.5),
        (r#"IFERROR(ABS("garbage"),7)"#, 7.0),
    ] {
        assert!(
            matches!(run_formula(&workbook, 0, AT, formula, context, EvaluationBudgets::default()), EvaluationOutcome::Complete { value: Number(n), .. } if n == expected),
            "{formula}"
        );
    }
    let frozen = EvaluationInputs {
        as_of_time: UNIX_EPOCH + Duration::from_secs(1704067200),
        ..context
    };
    assert!(
        matches!(run_formula(&workbook, 0, AT, r#"ABS("1 Jan")"#, frozen, EvaluationBudgets::default()), EvaluationOutcome::Complete { value: Number(n), .. } if n == 45292.0)
    );
    let mut session = Session::with_context(&workbook, frozen);
    let result = session.expr(0, AT, &expression(r#"ABS("1 Jan")"#)).unwrap();
    assert!(matches!(result, Value::Scalar(Number(n)) if n == 45292.0));
    assert!(session.numeric_date_effects[&(0, AT)].default_year);
    assert!(matches!(
        run_formula(
            &workbook,
            0,
            AT,
            r#"IFERROR(ABS("1 Jan"),7)"#,
            context,
            EvaluationBudgets::default()
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::UnsupportedCapability {
                capability: CapabilityId::DateParsing,
                ..
            },
            ..
        }
    ));
}
#[test]
fn numeric_datetime_math_arity_and_first_error_precedence() {
    let workbook = workbook();
    let context = EvaluationInputs {
        as_of_time: UNIX_EPOCH + Duration::from_secs(300_000_000_000),
        randomness_seed: 0,
        localization: Localization {
            utc_offset_seconds: 0,
            date_order: DateOrder::MonthDayYear,
        },
    };
    for formula in [
        r#"ABS("1 Jan",1/0)"#,
        r#"ROUND("1 Jan")"#,
        r#"LOG("1 Jan",1/0,1/0)"#,
        r#"TRUNC("1 Jan",1/0,1/0)"#,
    ] {
        assert!(
            matches!(
                run_formula(
                    &workbook,
                    0,
                    AT,
                    formula,
                    context,
                    EvaluationBudgets::default()
                ),
                EvaluationOutcome::Complete {
                    value: Error(CellError::Value),
                    ..
                }
            ),
            "{formula}"
        );
    }
    // Each first-operand spreadsheet failure must stop before the second
    // operand's operational missing-year context read.
    for formula in [
        r#"POWER("garbage","1 Jan")"#,
        r#"LOG("garbage","1 Jan")"#,
        r#"TRUNC("garbage","1 Jan")"#,
    ] {
        assert!(
            matches!(
                run_formula(
                    &workbook,
                    0,
                    AT,
                    formula,
                    context,
                    EvaluationBudgets::default()
                ),
                EvaluationOutcome::Complete {
                    value: Error(CellError::Value),
                    ..
                }
            ),
            "{formula}"
        );
    }
    for formula in [
        r#"POWER(1/0,"1 Jan")"#,
        r#"LOG(1/0,"1 Jan")"#,
        r#"TRUNC(1/0,"1 Jan")"#,
    ] {
        assert!(
            matches!(
                run_formula(
                    &workbook,
                    0,
                    AT,
                    formula,
                    context,
                    EvaluationBudgets::default()
                ),
                EvaluationOutcome::Complete {
                    value: Error(CellError::Div0),
                    ..
                }
            ),
            "{formula}"
        );
    }
    // Coercion of the first operand also precedes the second error.
    assert!(matches!(
        run_formula(
            &workbook,
            0,
            AT,
            r#"POWER("1 Jan",1/0)"#,
            context,
            EvaluationBudgets::default()
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::UnsupportedCapability {
                capability: CapabilityId::DateParsing,
                ..
            },
            ..
        }
    ));
    for (formula, expected) in [(r#"LOG("1 Jan 1900")"#, 0.0), (r#"TRUNC("12:00")"#, 0.0)] {
        assert!(
            matches!(run_formula(&workbook,0,AT,formula,context,EvaluationBudgets::default()), EvaluationOutcome::Complete { value: Number(n), .. } if n==expected),
            "{formula}"
        );
    }
    let mut session = Session::with_context(&workbook, context);
    let result = session
        .expr(0, AT, &expression(r#"LOG("garbage","1 Jan")"#))
        .unwrap();
    assert!(matches!(result, Value::Scalar(Error(CellError::Value))));
    assert!(
        session.numeric_date_effects.is_empty(),
        "untaken second coercion has no context effects"
    );
}
#[test]
fn composed_numeric_effects_emit_actual_context_fields() {
    use crate::run::{ContextRead, DependencyRead, DependencyReadSink, ReadEvent};
    #[derive(Default)]
    /// Collects context fields actually consumed by composed numeric coercion.
    struct Reads(Vec<DependencyRead>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event.read);
        }
    }
    let workbook = workbook();
    for (text, uses_clock) in [("1 Jan 2024", false), ("1 Jan", true)] {
        let mut reads = Reads::default();
        {
            let mut session = Session::with_context(
                &workbook,
                EvaluationInputs {
                    as_of_time: UNIX_EPOCH + Duration::from_secs(1704067200),
                    ..EvaluationInputs::default()
                },
            );
            session.dependency_sink = Some(&mut reads);
            let mut host = CallHost::new(&mut session, 0, AT, &[]);
            assert_eq!(
                host.coercev2_number_text(text, NumericPolicy::PinnedPoi)
                    .unwrap(),
                45292.
            );
        }
        assert_eq!(
            reads
                .0
                .contains(&DependencyRead::ContextField(ContextRead::AsOf)),
            uses_clock
        );
        assert!(
            reads
                .0
                .contains(&DependencyRead::ContextField(ContextRead::UtcOffset))
        );
    }
}

#[test]
fn array_consumer_memo_is_independent_of_projection_order_and_draws_once() {
    let mut workbook = workbook();
    workbook.sheet_mut(0).unwrap().set_value(AT, 2.).unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(CellAddress { row: 1, col: 0 }, 3.)
        .unwrap();
    let arguments = [expression("A1:A2*RAND()")];
    for scalar_first in [false, true] {
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        let mut host = CallHost::new_array(&mut session, 0, AT, &arguments, arguments.len());
        let first = if scalar_first {
            host.scalar_data(0, MissingPolicy::Blank).unwrap().value
        } else {
            assert_eq!(host.kind(0).unwrap(), OperandKind::Array);
            host.read_cell(0, 0, 0).unwrap().value
        };
        let evaluated = host.session.budget.usage.expressions;
        assert_eq!(host.shape(0).unwrap(), (2, 1));
        assert_eq!(
            host.scalar_data(0, MissingPolicy::Blank).unwrap().value,
            first
        );
        let Number(first) = first else {
            panic!("expected number")
        };
        let Number(second) = host.read_cell(0, 1, 0).unwrap().value else {
            panic!("expected number")
        };
        assert_eq!(second, first * 1.5);
        assert_eq!(host.session.budget.usage.expressions, evaluated);
        assert_eq!(
            host.session
                .budget
                .usage
                .random_accesses
                .iter()
                .map(|access| access.draws)
                .sum::<u64>(),
            1
        );
        assert!(matches!(
            host.session.expression_demand,
            expression_execution::ExpressionDemand::Scalar
        ));
    }
}

#[test]
fn array_demand_restores_after_operational_failure() {
    let workbook = workbook();
    let arguments = [expression("A1:B2*2")];
    let mut session = Session::new(
        &workbook,
        EvaluationInputs::default(),
        BudgetMeter::new(EvaluationBudgets {
            materialized_values: 3,
            ..Default::default()
        })
        .unwrap(),
    );
    {
        let mut host = CallHost::new_array(&mut session, 0, AT, &arguments, arguments.len());
        assert!(matches!(
            host.kind(0),
            Err(FunctionFailure::Operational(EvaluationFailure::Budget(_)))
        ));
        assert!(host.memo.is_empty());
    }
    assert!(matches!(
        session.expression_demand,
        expression_execution::ExpressionDemand::Scalar
    ));
}

#[test]
fn scalar_argument_roles_do_not_inherit_an_outer_array_request() {
    let mut workbook = workbook();
    workbook.sheet_mut(0).unwrap().set_value(AT, 1.).unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(CellAddress { row: 1, col: 0 }, 3.)
        .unwrap();
    let arguments = [expression("A1:A2>2")];
    for scalar_first in [false, true] {
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.expression_demand = expression_execution::ExpressionDemand::Array;
        let at = CellAddress { row: 1, col: 5 };
        let mut host = CallHost::new_array(&mut session, 0, at, &arguments, 0);
        if scalar_first {
            assert_eq!(
                host.scalar_data(0, MissingPolicy::Blank).unwrap().value,
                Boolean(true)
            );
        } else {
            assert_eq!(host.kind(0).unwrap(), OperandKind::Scalar);
        }
        let expressions = host.session.budget.usage.expressions;
        assert_eq!(host.shape(0).unwrap(), (1, 1));
        assert_eq!(
            host.scalar_data(0, MissingPolicy::Blank).unwrap().value,
            Boolean(true)
        );
        assert_eq!(host.session.budget.usage.expressions, expressions);
        assert_eq!(host.session.budget.usage.cell_reads, 1);
        assert!(matches!(
            host.session.expression_demand,
            expression_execution::ExpressionDemand::Array
        ));
    }
}

#[test]
fn index_memo_preserves_kind_shape_and_origin_in_both_projection_orders() {
    for formula in [
        "INDEX({1,2;3,4},0,1)",
        "INDEX(TRANSPOSE({1,2;3,4}),0,1)",
        "INDEX(A1:A2,0,1)",
    ] {
        for scalar_first in [false, true] {
            let mut workbook = workbook();
            workbook.sheet_mut(0).unwrap().set_value(AT, 1.).unwrap();
            let arguments = [expression(formula)];
            let mut session = Session::with_context(&workbook, EvaluationInputs::default());
            session.builtin_policy = BuiltinSemanticsPolicy::ExcelDocumentation;
            let mut host = CallHost::new(&mut session, 0, AT, &arguments);
            if scalar_first {
                assert_eq!(
                    host.scalar_data(0, MissingPolicy::Blank).unwrap().value,
                    Number(1.)
                );
            }
            let expected_kind = if formula.contains("A1:A2") {
                OperandKind::Reference
            } else {
                OperandKind::Array
            };
            assert_eq!(host.kind(0).unwrap(), expected_kind);
            let expressions = host.session.budget.usage.expressions;
            assert_eq!(host.shape(0).unwrap(), (2, 1));
            let scalar = host.scalar_data(0, MissingPolicy::Blank).unwrap();
            assert_eq!(scalar.value, Number(1.));
            assert_eq!(
                scalar.origin,
                if expected_kind == OperandKind::Reference {
                    OperandOrigin::ReferencedCell
                } else {
                    OperandOrigin::ArrayElement
                }
            );
            assert_eq!(host.kind(0).unwrap(), expected_kind);
            assert_eq!(host.session.budget.usage.expressions, expressions);
        }
    }
}

#[test]
fn index_scalar_selection_retains_origin_and_array_slice_anchor() {
    let workbook = workbook();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.builtin_policy = BuiltinSemanticsPolicy::ExcelDocumentation;
    for (formula, origin) in [
        ("INDEX(1,1,1)", OperandOrigin::Direct),
        ("INDEX({1,2},1,1)", OperandOrigin::ArrayElement),
        ("INDEX(INDEX({1,2},1,1),1,1)", OperandOrigin::ArrayElement),
    ] {
        let arguments = [expression(formula)];
        let mut host = CallHost::new(&mut session, 0, AT, &arguments);
        assert_eq!(host.kind(0).unwrap(), OperandKind::Scalar);
        assert_eq!(
            host.scalar_data(0, MissingPolicy::Blank).unwrap().origin,
            origin
        );
    }
    let at = CellAddress { row: 4, col: 5 };
    let plan = expression("INDEX({1,2;3,4},2,0)");
    match session.expr(0, at, &plan).unwrap() {
        Value::Array(view) => {
            // Literal arrays have established A1 coordinates, independent of the caller.
            // Selecting their second row preserves that origin plus the row offset.
            assert_eq!(view.anchor, Some(CellAddress { row: 1, col: 0 }));
            assert_eq!(view.values, vec![vec![Number(3.), Number(4.)]]);
        }
        _ => panic!("whole row selection must retain its array channel"),
    }
}
