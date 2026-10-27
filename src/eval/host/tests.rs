use super::argument_coercion::ArgumentCoercion;
use super::call_host::CallHost;
use super::cell_read_plan::CellReadPlan;
use super::classic_lookup_kind::ClassicLookupKind;
use super::context_reads::ContextReads;
use super::host_operand::HostOperand;
use super::lookup_service::LookupService;
use super::missing_policy::MissingPolicy;
use super::operand_kind::OperandKind;
use super::operand_reader::OperandReader;
use super::selection_access::SelectionAccess;
use super::traversal_policy::TraversalPolicy;
use crate::eval::adapters::search::execution::{
    LookupTable, SearchAxis, SearchMode, SessionSearch,
};
use crate::eval::compile::bound::Expression;
use crate::eval::config::{DateOrder, EvaluationInputs, Localization};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::entry::run_formula;
use crate::eval::limits::{
    BudgetError, BudgetKind, BudgetMeter, CapabilityId, EvalResult, EvaluationBudgets,
    EvaluationError, EvaluationOutcome,
};
use crate::eval::session::{
    ArgumentReader, Coercer, ItemFolder, ObservationRecorder, ResultProjection, Session,
};
use crate::eval::values::core_projection::checked_text_view;
use crate::eval::values::{
    Area, AreaKind, ArrayView, FunctionError, OperandOrigin, VALUE_ERROR, Value,
};
use crate::eval::vm::{
    ArgumentsRef, BranchSelector, ExpressionRunner, RectangularTraversal, Selection, Shape,
};
use crate::eval::{compile, session};
use crate::model::CellError::{NA, Ref};
use crate::model::CellValue::{Blank, Boolean, Error, Number, Text};
use crate::model::{CellAddress, CellError, CellValue, MAX_COLUMNS, MAX_ROWS, Workbook};
use poirot_functions::coercion::compatibility::NumericTextDecision as CoercionDecision;
use std::time::{Duration, UNIX_EPOCH};

const AT: CellAddress = CellAddress { row: 0, col: 0 };

fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
}

fn expression(formula: &str) -> Expression {
    compile::bound::bind(&poirot_parsing::formula::parse_formula(formula).unwrap())
}

#[test]
fn one_shot_aggregate_arguments_do_not_accumulate_vm_slots() {
    let workbook = workbook();
    let arguments = [
        expression("1"),
        expression("2"),
        expression("3"),
        expression("4"),
    ];
    let mut session = Session::new(
        &workbook,
        EvaluationInputs::default(),
        BudgetMeter::new(EvaluationBudgets {
            materialized_values: 1,
            ..EvaluationBudgets::default()
        })
        .unwrap(),
    );
    let mut host = CallHost::new_inheriting(&mut session, 0, AT, &arguments);
    let mut values = Vec::new();
    for index in 0..arguments.len() {
        values.push(host.aggregate_argument_once(index).unwrap().into_value());
    }
    assert!(values.into_iter().zip([1., 2., 3., 4.]).all(
        |(value, expected)| matches!(value, Value::Scalar(Number(number)) if number == expected)
    ));
    assert_eq!(host.argument_slots, 0);
    assert_eq!(
        host.session.state.runtime.budget.usage.materialized_values,
        0
    );
    assert!(matches!(
        host.aggregate_argument_once(4),
        Err(FunctionError::Spreadsheet(CellError::Value))
    ));
}

#[test]
fn one_shot_blank_references_keep_median_budget_one_contract() {
    let workbook = workbook();
    let arguments = [expression("A1"), expression("A2"), expression("A3")];
    let budgets = EvaluationBudgets {
        materialized_values: 1,
        ..EvaluationBudgets::default()
    };
    let mut session = Session::new(
        &workbook,
        EvaluationInputs::default(),
        BudgetMeter::new(budgets).unwrap(),
    );
    let mut host = CallHost::new_inheriting(&mut session, 0, AT, &arguments);
    for index in 0..arguments.len() {
        let operand = host.aggregate_argument_once(index).unwrap();
        assert!(matches!(operand.value(), Value::Refs(_)));
    }
    assert_eq!(host.argument_slots, 0);
    // Each reference charges one descriptor; the collection limit of one is
    // checked separately from cumulative usage across all three arguments.
    assert_eq!(
        host.session.state.runtime.budget.usage.materialized_values,
        3
    );
}

#[test]
fn selection_buffer_and_sparse_countblank_preserve_limits_and_logical_area() {
    let mut workbook = workbook();
    workbook.sheet_mut(0).unwrap().set_value("A1", 3.).unwrap();
    let arguments = [expression("A1:A3")];
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    let value = host.aggregate_argument_once(0).unwrap().into_value();
    assert_eq!(
        OperandReader::new(&mut host)
            .countblank_references(&value)
            .unwrap(),
        Some(2)
    );
    assert_eq!(host.session.state.runtime.budget.usage.cell_reads, 1);
    assert_eq!(
        OperandReader::new(&mut host)
            .countblank_references(&Value::Scalar(Number(1.)))
            .unwrap(),
        None
    );
    let selected = SelectionAccess::new(&mut host)
        .buffer(
            2,
            Selection::Pending,
            crate::eval::limits::EvaluationReasonError::SelectionAllocationFailed,
        )
        .unwrap();
    assert_eq!(selected.len(), 2);
    assert!(
        selected
            .iter()
            .all(|slot| matches!(slot, Selection::Pending))
    );
    let before = host.session.state.runtime.budget.usage.helper_operations;
    host.charge_work(3).unwrap();
    assert_eq!(
        host.session.state.runtime.budget.usage.helper_operations - before,
        3
    );

    let mut limited = Session::new(
        &workbook,
        EvaluationInputs::default(),
        BudgetMeter::new(EvaluationBudgets {
            materialized_values: 1,
            ..EvaluationBudgets::default()
        })
        .unwrap(),
    );
    let mut host = CallHost::new(&mut limited, 0, AT, &[]);
    assert!(matches!(
        SelectionAccess::new(&mut host).buffer(
            2,
            Selection::Pending,
            crate::eval::limits::EvaluationReasonError::SelectionAllocationFailed
        ),
        Err(EvaluationError::Budget(_))
    ));
}

#[test]
fn aggregate_fold_retains_origin_omission_and_batch_usage() {
    let workbook = workbook();
    let arguments = [expression("{1,2,3}")];
    let mut baseline = Session::with_context(&workbook, EvaluationInputs::default());
    let mut expected = Vec::new();
    let operand = HostOperand {
        value: Value::Array(ArrayView::unknown(vec![vec![
            Number(1.),
            Blank,
            Number(3.),
        ]])),
        missing: false,
        array_missing: vec![vec![false, true, false]],
    };
    ItemFolder::new(&mut baseline)
        .fold_items(
            operand,
            (),
            |_, _| Ok(()),
            |_, member| {
                expected.push((member.value, member.origin, member.missing));
                Ok(())
            },
        )
        .unwrap();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    let operand = HostOperand {
        value: Value::Array(ArrayView::unknown(vec![vec![
            Number(1.),
            Blank,
            Number(3.),
        ]])),
        missing: false,
        array_missing: vec![vec![false, true, false]],
    };
    let mut actual = Vec::new();
    host.fold_aggregate_operand(
        operand,
        (),
        |_, _| Ok(()),
        |_, member| {
            actual.push((member.value, member.origin, member.missing));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 3);
    assert_eq!(actual[1], (Blank, OperandOrigin::ArrayElement, true));
    assert_eq!(
        host.session.state.runtime.budget.usage,
        baseline.state.runtime.budget.usage
    );
    assert!(
        actual
            .iter()
            .all(|(_, origin, _)| *origin == OperandOrigin::ArrayElement)
    );
}

#[test]
fn ordered_sheet_span_stops_before_later_cell_read() {
    let mut workbook = workbook();
    workbook.add_sheet("Sheet2").unwrap();
    workbook.sheet_mut(0).unwrap().set_value("A1", 7.).unwrap();
    workbook
        .sheet_mut(1)
        .unwrap()
        .set_formula("A1", "2")
        .unwrap();
    let arguments = [expression("Sheet1:Sheet2!A1")];
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    let operand = host.argument(0).unwrap();
    let values = host
        .fold_ordered_sheet_span(operand, Vec::new(), |values, member| {
            values.push((member.value, member.origin, member.missing));
            Ok(std::ops::ControlFlow::Break(()))
        })
        .unwrap();
    assert_eq!(
        values,
        vec![(Number(7.), OperandOrigin::ReferencedCell, false)]
    );
    assert_eq!(host.session.state.runtime.budget.usage.cell_reads, 1);
}

#[test]
fn eager_aggregate_fold_reads_after_first_semantic_error() {
    use crate::eval::observe::{ExecutionEvent, ExecutionObserver};
    /// Reject the later formula body after the first member produced a spreadsheet error.
    struct FailLater;
    impl ExecutionObserver for FailLater {
        fn observe(&mut self, event: ExecutionEvent<'_>) -> EvalResult<()> {
            if let ExecutionEvent::Begin(operation) = event
                && operation.target.address == (CellAddress { row: 0, col: 1 })
            {
                return Err(EvaluationError::StalePlan);
            }
            Ok(())
        }
    }
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula("A1", "1/0")
        .unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula("B1", "2")
        .unwrap();
    let arguments = [expression("A1:B1")];
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut observer = FailLater;
    session.ports.execution_observer = Some(&mut observer);
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    let operand = host.argument(0).unwrap();
    let mut first_error = None;
    let result = host.fold_aggregate_operand(
        operand,
        (),
        |_, _| Ok(()),
        |_, member| {
            if first_error.is_none() && matches!(member.value, Error(_)) {
                first_error = Some(member.value);
            }
            Ok(())
        },
    );
    assert!(first_error.is_some());
    assert!(
        matches!(result, Err(EvaluationError::StalePlan)),
        "{result:?}"
    );
}

#[test]
fn selected_transfer_matches_session_observation_and_budget() {
    use crate::eval::observe::{ExecutionEvent, ExecutionObserver, OperationKind};
    /// Records argument entry and completion order to compare selected transfers with ordinary evaluation.
    struct Events(Vec<(bool, usize)>);
    impl ExecutionObserver for Events {
        fn observe(&mut self, event: ExecutionEvent<'_>) -> EvalResult<()> {
            match event {
                ExecutionEvent::Begin(operation) => {
                    if let OperationKind::Argument { ordinal, .. } = operation.kind {
                        self.0.push((true, ordinal));
                    }
                }
                ExecutionEvent::Complete { operation, .. } => {
                    if let OperationKind::Argument { ordinal, .. } = operation.kind {
                        self.0.push((false, ordinal));
                    }
                }
                ExecutionEvent::MaskDecision { .. } => {}
            }
            Ok(())
        }
    }
    let workbook = workbook();
    let arguments = [expression("{1,2}")];
    let mut baseline_events = Events(Vec::new());
    let mut baseline = Session::with_context(&workbook, EvaluationInputs::default());
    baseline.ports.execution_observer = Some(&mut baseline_events);
    let (expected_id, expected) = ObservationRecorder::new(&mut baseline)
        .observed_value_argument(0, AT, (&arguments[0]).into(), 0)
        .unwrap();
    let expected_shape = Shape::inspect(&expected).unwrap();
    let expected_usage = baseline.state.runtime.budget.usage.clone();
    let mut host_events = Events(Vec::new());
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.ports.execution_observer = Some(&mut host_events);
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    let (actual_id, actual) = SelectionAccess::new(&mut host).argument(0).unwrap();
    assert_eq!(actual_id, expected_id);
    assert_eq!(
        Shape::inspect(&actual).unwrap().columns,
        expected_shape.columns
    );
    assert_eq!(host.session.state.runtime.budget.usage, expected_usage);
    drop(host);
    assert_eq!(host_events.0, baseline_events.0);
    assert_eq!(host_events.0, vec![(true, 0), (false, 0)]);
}

#[test]
fn selected_results_bridge_matches_session_for_choose_ifs_and_switch_controls() {
    use crate::eval::observe::{ExecutionEvent, ExecutionObserver, OperationKind};
    /// Argument events collected without changing selected branch evaluation.
    struct Events(Vec<(bool, usize)>);
    impl ExecutionObserver for Events {
        fn observe(&mut self, event: ExecutionEvent<'_>) -> EvalResult<()> {
            match event {
                ExecutionEvent::Begin(operation) => {
                    if let OperationKind::Argument { ordinal, .. } = operation.kind {
                        self.0.push((true, ordinal));
                    }
                }
                ExecutionEvent::Complete { operation, .. } => {
                    if let OperationKind::Argument { ordinal, .. } = operation.kind {
                        self.0.push((false, ordinal));
                    }
                }
                ExecutionEvent::MaskDecision { .. } => {}
            }
            Ok(())
        }
    }

    let mut workbook = workbook();
    for (address, value) in [("B1", 7.), ("C1", 8.), ("D1", 9.)] {
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value(address, value)
            .unwrap();
    }
    let cases = [
        (
            "CHOOSE",
            vec!["1", "B1:C1", "D1", "UNKNOWN_FUNCTION()"],
            vec![Selection::Branch(1), Selection::Branch(2)],
            vec![1, 2, 3],
            vec![Number(7.), Number(9.)],
            vec![(true, 1), (false, 1), (true, 2), (false, 2)],
        ),
        (
            "IFS",
            vec!["TRUE", "B1:C1", "FALSE", "UNKNOWN_FUNCTION()"],
            vec![Selection::Branch(1), Selection::Pending],
            vec![1, 3],
            vec![Number(7.), Error(NA)],
            vec![(true, 1), (false, 1)],
        ),
        (
            "SWITCH",
            vec!["1", "1", "B1:C1", "2", "UNKNOWN_FUNCTION()", "D1"],
            vec![Selection::Branch(2), Selection::Branch(5)],
            vec![2, 4, 5],
            vec![Number(7.), Number(9.)],
            vec![(true, 2), (false, 2), (true, 5), (false, 5)],
        ),
    ];
    for (name, formulas, selected, ordinals, expected_values, expected_events) in cases {
        let arguments: Vec<_> = formulas.into_iter().map(expression).collect();
        let shape = Shape {
            rows: 1,
            columns: 2,
        };
        let mut baseline_events = Events(Vec::new());
        let mut baseline = Session::with_context(&workbook, EvaluationInputs::default());
        baseline.ports.execution_observer = Some(&mut baseline_events);
        let expected = BranchSelector::new(&mut baseline)
            .selected_array_results(
                0,
                AT,
                (&arguments).into(),
                selected.clone(),
                shape,
                ordinals.iter().copied(),
            )
            .unwrap();
        let expected_usage = baseline.state.runtime.budget.usage.clone();

        let mut host_events = Events(Vec::new());
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.ports.execution_observer = Some(&mut host_events);
        let mut host = CallHost::new(&mut session, 0, AT, &arguments);
        let actual = SelectionAccess::new(&mut host)
            .array_results(selected, shape, ordinals.into_iter())
            .unwrap();
        let (Value::Array(actual), Value::Array(expected)) = (actual, expected) else {
            panic!("{name} should produce arrays");
        };
        assert_eq!(actual.values, expected.values, "{name}");
        assert_eq!(actual.values, vec![expected_values], "{name}");
        assert_eq!(
            host.session.state.runtime.budget.usage, expected_usage,
            "{name}"
        );
        drop(host);
        assert_eq!(host_events.0, baseline_events.0, "{name}");
        assert_eq!(host_events.0, expected_events, "{name}");
    }
}

#[test]
fn selected_results_bridge_preserves_resource_failure_and_usage() {
    let workbook = workbook();
    let arguments = [expression("1"), expression("{7,8,9}"), expression("2")];
    let shape = Shape {
        rows: 1,
        columns: 1,
    };
    let limits = EvaluationBudgets {
        materialized_values: 2,
        ..Default::default()
    };
    let mut baseline = Session::with_context(&workbook, EvaluationInputs::default());
    baseline.state.runtime.budget = BudgetMeter::new(limits).unwrap();
    let expected = BranchSelector::new(&mut baseline).selected_array_results(
        0,
        AT,
        (&arguments).into(),
        vec![Selection::Branch(1)],
        shape,
        1..2,
    );
    let expected_usage = baseline.state.runtime.budget.usage.clone();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.state.runtime.budget = BudgetMeter::new(limits).unwrap();
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    let actual =
        SelectionAccess::new(&mut host).array_results(vec![Selection::Branch(1)], shape, 1..2);
    assert!(matches!(expected, Err(EvaluationError::Budget(_))));
    match (actual, expected) {
        (Err(actual), Err(expected)) => assert_eq!(actual.budget(), expected.budget()),
        _ => panic!("selected result resource failure differs"),
    }
    assert_eq!(host.session.state.runtime.budget.usage, expected_usage);
}

#[test]
fn selected_member_and_array_preserve_reference_laziness_and_usage() {
    let mut workbook = workbook();
    workbook.sheet_mut(0).unwrap().set_value("B1", 7.).unwrap();
    workbook.sheet_mut(0).unwrap().set_value("C1", 8.).unwrap();
    let arguments = [expression("B1:C1"), expression("UNKNOWN_FUNCTION()")];
    let mut baseline = Session::with_context(&workbook, EvaluationInputs::default());
    let (_, expected) = ObservationRecorder::new(&mut baseline)
        .observed_value_argument(0, AT, (&arguments[0]).into(), 0)
        .unwrap();
    let shape = Shape::inspect(&expected).unwrap();
    let expected_array = RectangularTraversal::new(&mut baseline)
        .values(AT, shape, |session, row, column| {
            if column == 0 {
                Ok(Number(11.))
            } else {
                RectangularTraversal::new(session).element(&expected, shape, row, column)
            }
        })
        .unwrap();
    let expected_usage = baseline.state.runtime.budget.usage.clone();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    let (_, value) = SelectionAccess::new(&mut host).argument(0).unwrap();
    assert_eq!(host.session.state.runtime.budget.usage.cell_reads, 0);
    let output = SelectionAccess::new(&mut host)
        .array(shape, |reader, row, column| {
            if column == 0 {
                Ok(Number(11.))
            } else {
                reader.member(&value, shape, row, column)
            }
        })
        .unwrap();
    let (Value::Array(actual), Value::Array(expected)) = (output, expected_array) else {
        panic!("expected arrays")
    };
    assert_eq!(actual.values, expected.values);
    assert_eq!(host.session.state.runtime.budget.usage, expected_usage);
    assert_eq!(
        SelectionAccess::new(&mut host)
            .member(&value, shape, 0, 0)
            .unwrap(),
        Number(7.)
    );
}

#[test]
fn selected_transfer_emits_failure_completion_and_scalar_forwarder_keeps_policy() {
    use crate::eval::observe::{ExecutionEvent, ExecutionObserver, OperationKind};
    /// Records argument entry and failed completion to verify transfer failures keep their observation pair.
    struct Events(Vec<bool>);
    impl ExecutionObserver for Events {
        fn observe(&mut self, event: ExecutionEvent<'_>) -> EvalResult<()> {
            match event {
                ExecutionEvent::Begin(operation)
                    if matches!(operation.kind, OperationKind::Argument { ordinal: 0, .. }) =>
                {
                    self.0.push(true);
                }
                ExecutionEvent::Complete { operation, result }
                    if matches!(operation.kind, OperationKind::Argument { ordinal: 0, .. }) =>
                {
                    assert!(result.is_err());
                    self.0.push(false);
                }
                _ => {}
            }
            Ok(())
        }
    }
    let workbook = workbook();
    let arguments = [expression("1+1")];
    let mut events = Events(Vec::new());
    let mut session = Session::new(
        &workbook,
        EvaluationInputs::default(),
        BudgetMeter::new(EvaluationBudgets {
            run_work: 0,
            ..EvaluationBudgets::default()
        })
        .unwrap(),
    );
    session.ports.execution_observer = Some(&mut events);
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    assert!(matches!(
        SelectionAccess::new(&mut host).argument(0),
        Err(EvaluationError::Budget(_))
    ));
    drop(host);
    assert_eq!(events.0, [true, false]);

    let mut baseline = Session::with_context(&workbook, EvaluationInputs::default());
    let expected = ArgumentReader::new(&mut baseline)
        .argument(0, AT, (&arguments).into(), 0)
        .unwrap();
    let expected_usage = baseline.state.runtime.budget.usage.clone();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    assert_eq!(host.scalar_argument(0).unwrap(), expected);
    assert_eq!(host.session.state.runtime.budget.usage, expected_usage);
}

#[test]
fn selected_policy_omission_projection_and_choose_number_match_session() {
    use crate::eval::ports::{DependencyRead, DependencyReadSink, ReadEvent};
    #[derive(Default)]
    /// Records dependencies consumed by the selected invocation.
    struct Reads(Vec<DependencyRead>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event.read);
        }
    }
    let workbook = workbook();
    let arguments = [
        Expression::Missing,
        expression("{1,2}"),
        expression("UNKNOWN_FUNCTION()"),
    ];
    let number = Text("2".into());
    let mut baseline_reads = Reads::default();
    let mut baseline = Session::with_context(&workbook, EvaluationInputs::default());
    baseline.ports.dependency_sink = Some(&mut baseline_reads);
    let (_, expected_value) = ObservationRecorder::new(&mut baseline)
        .observed_value_argument(0, AT, (&arguments[1]).into(), 1)
        .unwrap();
    let expected_array = baseline
        .state
        .expression_demand
        .retains_array(&expected_value);
    let expected_scalar = ResultProjection::new(&mut baseline)
        .select_legacy_scalar(AT, expected_value)
        .unwrap();
    let expected_number = Coercer::new(&mut baseline)
        .coerce_scalar_number(&number)
        .unwrap();
    let expected_usage = baseline.state.runtime.budget.usage.clone();

    let mut host_reads = Reads::default();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.ports.dependency_sink = Some(&mut host_reads);
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    assert!(SelectionAccess::new(&mut host).is_missing(0));
    assert!(!SelectionAccess::new(&mut host).is_missing(1));
    assert!(!SelectionAccess::new(&mut host).is_missing(3));

    let (_, value) = SelectionAccess::new(&mut host).argument(1).unwrap();
    assert_eq!(
        SelectionAccess::new(&mut host).retains_array(&value),
        expected_array
    );
    assert_eq!(host.select_legacy_scalar(value).unwrap(), expected_scalar);
    assert_eq!(
        SelectionAccess::new(&mut host).number(&number).unwrap(),
        expected_number
    );
    assert_eq!(host.session.state.runtime.budget.usage, expected_usage);
    drop(host);
    assert_eq!(host_reads.0, baseline_reads.0);
}

#[test]
fn demand_memoization_preserves_references_without_reading_cells() {
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value(AT, "1/0", Number(99.))
        .unwrap();
    let arguments = [expression("A1"), expression("UNKNOWN_FUNCTION()")];
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    assert_eq!(host.kind(0).unwrap(), OperandKind::Reference);
    let expression_count = host.session.state.runtime.budget.usage.expressions;
    assert_eq!(host.kind(0).unwrap(), OperandKind::Reference);
    assert_eq!(
        host.session.state.runtime.budget.usage.expressions,
        expression_count
    );
    assert_eq!(host.session.state.runtime.budget.usage.cell_reads, 0);
    assert_eq!(host.prepared_arguments, 1);
    assert_eq!(
        host.scalar_data(0, MissingPolicy::Reject).unwrap().value,
        Error(CellError::Div0)
    );
    assert_eq!(host.prepared_arguments, 1);
}

#[test]
fn missing_and_blank_and_array_missing_are_distinct() {
    let workbook = workbook();
    let arguments = [
        Expression::Missing,
        expression("A1"),
        Expression::Array(vec![
            vec![Expression::Number(1.), Expression::Missing],
            vec![Expression::Missing, Expression::Number(2.)],
        ]),
    ];
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    assert_eq!(host.kind(0).unwrap(), OperandKind::Missing);
    assert!(matches!(
        host.scalar_data(0, MissingPolicy::Reject),
        Err(FunctionError::Spreadsheet(CellError::Value))
    ));
    assert_eq!(
        host.scalar_data(0, MissingPolicy::Zero).unwrap().value,
        Number(0.)
    );
    assert_eq!(
        host.scalar_data(0, MissingPolicy::Blank).unwrap().value,
        Blank
    );
    let blank = host.scalar_data(1, MissingPolicy::Reject).unwrap();
    assert_eq!(blank.value, Blank);
    assert!(!blank.missing);
    assert_eq!(blank.origin, OperandOrigin::ReferencedCell);
    assert_eq!(
        host.argument(2).unwrap().array_missing(),
        &[vec![false, true], vec![true, false]]
    );
    let sparse = TraversalPolicy::SparseAggregation
        .collect(&mut host, 2)
        .unwrap();
    assert_eq!(sparse.len(), 4);
    assert_eq!(
        sparse
            .iter()
            .map(|member| member.missing)
            .collect::<Vec<_>>(),
        [false, true, true, false]
    );
    assert!(
        sparse
            .iter()
            .all(|member| member.origin == OperandOrigin::ArrayElement)
    );
    let omitted = TraversalPolicy::SparseAggregation
        .collect(&mut host, 0)
        .unwrap();
    assert_eq!(omitted.len(), 1);
    assert!(omitted[0].missing);
    assert_eq!(omitted[0].value, Blank);
    let last = TraversalPolicy::LastAreaRow.collect(&mut host, 2).unwrap();
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
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(CellAddress { row: 1, col: 1 }, 5.)
        .unwrap();
    let arguments = [expression("(A1,B2)")];
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    assert!(matches!(
        TraversalPolicy::DenseRectangle.collect(&mut host, 0),
        Err(FunctionError::Spreadsheet(CellError::Value))
    ));
    let sparse = TraversalPolicy::SparseAggregation
        .collect(&mut host, 0)
        .unwrap();
    assert_eq!(sparse.len(), 2);
    assert_eq!(sparse[0].value, Number(3.));
    assert_eq!(sparse[0].origin, OperandOrigin::ReferencedCell);
    assert_eq!(sparse[1].value, Number(5.));
    assert_eq!(sparse[1].origin, OperandOrigin::ReferencedCell);
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
    assert_eq!(host.session.policies.context.randomness_seed, 17);
    assert_eq!(host.session.policies.context.as_of_time, UNIX_EPOCH);
    assert_eq!(
        ContextReads::new(&mut host).date_system_snapshot(),
        workbook.date_system
    );
    assert_eq!(host.target().address, AT);
    assert_eq!(host.remaining_work(), 2);
    host.charge_work(2).unwrap();
    assert_eq!(host.remaining_work(), 0);
    assert!(matches!(
        host.charge_work(1),
        Err(EvaluationError::Budget(BudgetError {
            kind: BudgetKind::RunWork,
            ..
        }))
    ));
    assert_eq!(host.session.state.runtime.budget.usage.helper_operations, 2);
}

#[test]
fn budget_capacity_checks_do_not_charge_but_admission_does() {
    let workbook = workbook();
    let mut session = Session::new(
        &workbook,
        EvaluationInputs::default(),
        BudgetMeter::new(EvaluationBudgets {
            run_work: 10,
            materialized_values: 3,
            text_bytes: 4,
            ..EvaluationBudgets::default()
        })
        .unwrap(),
    );
    let mut host = CallHost::new(&mut session, 0, AT, &[]);
    let values_capacity: EvalResult<()> = host.check_values_capacity(3);
    let text_capacity: EvalResult<()> = host.check_text_capacity(4);
    values_capacity.unwrap();
    text_capacity.unwrap();
    assert_eq!(host.remaining_work(), 10);
    assert_eq!(
        host.session.state.runtime.budget.usage.materialized_values,
        0
    );
    assert_eq!(host.session.state.runtime.budget.usage.text_bytes, 0);

    let values: EvalResult<()> = host.check_values(3);
    values.unwrap();
    assert_eq!(host.remaining_work(), 7);
    assert_eq!(
        host.session.state.runtime.budget.usage.materialized_values,
        3
    );
    let text: EvalResult<()> = host.check_text(4);
    text.unwrap();
    assert_eq!(host.remaining_work(), 3);
    assert_eq!(host.session.state.runtime.budget.usage.text_bytes, 4);
    let work: EvalResult<()> = host.charge_work(3);
    work.unwrap();
    assert_eq!(host.remaining_work(), 0);
    assert_eq!(host.session.state.runtime.budget.usage.helper_operations, 3);
    host.check_values_capacity(3).unwrap();
    host.check_text_capacity(4).unwrap();
    assert_eq!(host.session.state.runtime.budget.usage.work_units, 10);
}

#[test]
fn budget_admission_rejects_capacity_before_work_without_charging() {
    let workbook = workbook();
    let mut session = Session::new(
        &workbook,
        EvaluationInputs::default(),
        BudgetMeter::new(EvaluationBudgets {
            run_work: 0,
            materialized_values: 1,
            text_bytes: 1,
            ..EvaluationBudgets::default()
        })
        .unwrap(),
    );
    let mut host = CallHost::new(&mut session, 0, AT, &[]);
    for (result, expected) in [
        (host.check_values(2), BudgetKind::Materialization),
        (host.check_values_capacity(2), BudgetKind::Materialization),
        (host.check_text(2), BudgetKind::TextBytes),
        (host.check_text_capacity(2), BudgetKind::TextBytes),
        (host.check_values(1), BudgetKind::RunWork),
        (host.check_text(1), BudgetKind::RunWork),
        (host.charge_work(1), BudgetKind::RunWork),
    ] {
        let failure = result.unwrap_err();
        assert_eq!(failure.budget().unwrap().kind, expected);
    }
    assert_eq!(host.session.state.runtime.budget.usage.work_units, 0);
    assert_eq!(
        host.session.state.runtime.budget.usage.materialized_values,
        0
    );
    assert_eq!(host.session.state.runtime.budget.usage.text_bytes, 0);
    assert_eq!(host.session.state.runtime.budget.usage.helper_operations, 0);
}

#[test]
fn nested_selected_missing_is_not_a_blank_reference() {
    let workbook = workbook();
    let arguments = [
        expression("IFS(TRUE,)"),
        expression("A1"),
        Expression::Missing,
    ];
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    for index in [0, 2] {
        assert_eq!(host.kind(index).unwrap(), OperandKind::Missing);
        assert!(
            host.scalar_data(index, MissingPolicy::Blank)
                .unwrap()
                .missing
        );
        assert!(
            OperandReader::new(&mut host)
                .read_cell(index, 0, 0)
                .unwrap()
                .missing
        );
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
    assert!(!OperandReader::new(&mut host).describe(0).unwrap().is_area);
    assert!(OperandReader::new(&mut host).describe(1).unwrap().is_area);
    assert!(OperandReader::new(&mut host).describe(2).unwrap().is_area);
    assert_eq!(host.session.state.runtime.budget.usage.cell_reads, 0);
    assert!(matches!(
        OperandReader::new(&mut host).read_cell(1, 1, 0),
        Err(FunctionError::Spreadsheet(Ref))
    ));
    assert_eq!(
        OperandReader::new(&mut host)
            .read_relative_cell(1, 1, 0)
            .unwrap()
            .value,
        Number(42.)
    );
    assert_eq!(host.session.state.runtime.budget.usage.cell_reads, 1);
    assert!(matches!(
        OperandReader::new(&mut host).read_relative_cell(2, 1, 0),
        Err(FunctionError::Spreadsheet(Ref))
    ));
    assert!(matches!(
        OperandReader::new(&mut host).read_relative_cell(1, MAX_ROWS as usize, 0),
        Err(FunctionError::Spreadsheet(Ref))
    ));
}

#[test]
fn numeric_conversion_preserves_inputs_and_source_precision() {
    let workbook = workbook();
    let arguments = [expression("2.9999999999999996"), expression(r#"" 1e2 ""#)];
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let mut host = CallHost::new(&mut session, 0, AT, &arguments);
    let operand = host.scalar_data(0, MissingPolicy::Reject).unwrap();
    let n = ArgumentCoercion::new(&mut host)
        .coerce_scalar_number(&operand.value)
        .unwrap();
    assert_eq!(
        n.to_bits(),
        2.9999999999999996f64.to_bits(),
        "bound and scalar host preserve literal bits"
    );
    assert_eq!(n.trunc(), 2.);
    assert_eq!(
        poirot_functions::coercion::compatibility::coerce_number_to_text(n),
        "3",
        "same dependency and exact host input"
    );
    assert_eq!(
        ArgumentCoercion::new(&mut host)
            .coerce_number_to_text(n)
            .unwrap(),
        "3"
    );
    let operand = host.scalar_data(1, MissingPolicy::Reject).unwrap();
    assert_eq!(
        ArgumentCoercion::new(&mut host)
            .coerce_scalar_number(&operand.value)
            .unwrap(),
        100.
    );
    assert!(
        ArgumentCoercion::new(&mut host)
            .equal(&Number(n), &Number(3.))
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
            ArgumentCoercion::new(&mut host)
                .coerce_number_text("1 Jan 2024")
                .unwrap(),
            45292.0
        );
        assert_eq!(
            ArgumentCoercion::new(&mut host)
                .coerce_number_text("0:30 PM")
                .unwrap(),
            45000.0 / 86400.0
        );
        assert!(matches!(
            ArgumentCoercion::new(&mut host).coerce_number_text("garbage"),
            Err(FunctionError::Spreadsheet(CellError::Value))
        ));
        assert!(!host.session.state.runtime.effects.numeric_date_effects[&(0, AT)].default_year);
        assert!(matches!(
            ArgumentCoercion::new(&mut host).coerce_number_text("1 Jan"),
            Err(FunctionError::Operational(
                EvaluationError::UnsupportedCapability {
                    capability: CapabilityId::DateParsing,
                    ..
                }
            ))
        ));
    }
    assert_eq!(
        session.state.runtime.effects.numeric_date_effects[&(0, AT)],
        poirot_functions::coercion::date_time::Effects {
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
        ArgumentCoercion::new(&mut host)
            .coerce_number_text("1 Jan")
            .unwrap(),
        45292.0
    );
}

#[test]
fn numeric_datetime_actual_math_bridge_and_iferror_boundary() {
    let mut workbook = workbook();
    // Fallback remains source-1900 even when the workbook is date1904.
    workbook.date_system = crate::model::DateSystem::Excel1904;
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
    let result = ExpressionRunner::new(&mut session)
        .expr(0, AT, &expression(r#"ABS("1 Jan")"#))
        .unwrap();
    assert!(matches!(result, Value::Scalar(Number(n)) if n == 45292.0));
    assert!(session.state.runtime.effects.numeric_date_effects[&(0, AT)].default_year);
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
            failure: EvaluationError::UnsupportedCapability {
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
            failure: EvaluationError::UnsupportedCapability {
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
    let result = ExpressionRunner::new(&mut session)
        .expr(0, AT, &expression(r#"LOG("garbage","1 Jan")"#))
        .unwrap();
    assert!(matches!(result, Value::Scalar(Error(CellError::Value))));
    assert!(
        session
            .state
            .runtime
            .effects
            .numeric_date_effects
            .is_empty(),
        "untaken second coercion has no context effects"
    );
}

#[test]
fn composed_numeric_effects_emit_actual_context_fields() {
    use crate::eval::ports::{ContextRead, DependencyRead, DependencyReadSink, ReadEvent};
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
            session.ports.dependency_sink = Some(&mut reads);
            let mut host = CallHost::new(&mut session, 0, AT, &[]);
            assert_eq!(
                ArgumentCoercion::new(&mut host)
                    .coerce_number_text(text)
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
            OperandReader::new(&mut host)
                .read_cell(0, 0, 0)
                .unwrap()
                .value
        };
        let evaluated = host.session.state.runtime.budget.usage.expressions;
        assert_eq!(OperandReader::new(&mut host).shape(0).unwrap(), (2, 1));
        assert_eq!(
            host.scalar_data(0, MissingPolicy::Blank).unwrap().value,
            first
        );
        let Number(first) = first else {
            panic!("expected number")
        };
        let Number(second) = OperandReader::new(&mut host)
            .read_cell(0, 1, 0)
            .unwrap()
            .value
        else {
            panic!("expected number")
        };
        assert_eq!(second, first * 1.5);
        assert_eq!(
            host.session.state.runtime.budget.usage.expressions,
            evaluated
        );
        assert_eq!(
            host.session
                .state
                .runtime
                .budget
                .usage
                .random_accesses
                .iter()
                .map(|access| access.draws)
                .sum::<u64>(),
            1
        );
        assert!(matches!(
            host.session.state.expression_demand,
            session::ExpressionDemand::Scalar
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
            Err(FunctionError::Operational(EvaluationError::Budget(_)))
        ));
        assert!(host.prepared_arguments == 0);
    }
    assert!(matches!(
        session.state.expression_demand,
        session::ExpressionDemand::Scalar
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
        session.state.expression_demand = session::ExpressionDemand::Array;
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
        let expressions = host.session.state.runtime.budget.usage.expressions;
        assert_eq!(OperandReader::new(&mut host).shape(0).unwrap(), (1, 1));
        assert_eq!(
            host.scalar_data(0, MissingPolicy::Blank).unwrap().value,
            Boolean(true)
        );
        assert_eq!(
            host.session.state.runtime.budget.usage.expressions,
            expressions
        );
        assert_eq!(host.session.state.runtime.budget.usage.cell_reads, 1);
        assert!(matches!(
            host.session.state.expression_demand,
            session::ExpressionDemand::Array
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
            let expressions = host.session.state.runtime.budget.usage.expressions;
            assert_eq!(OperandReader::new(&mut host).shape(0).unwrap(), (2, 1));
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
            assert_eq!(
                host.session.state.runtime.budget.usage.expressions,
                expressions
            );
        }
    }
}

#[test]
fn index_scalar_selection_retains_origin_and_array_slice_anchor() {
    let workbook = workbook();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    {
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
    }
    let at = CellAddress { row: 4, col: 5 };
    let plan = expression("INDEX({1,2;3,4},2,0)");
    match ExpressionRunner::new(&mut session)
        .expr(0, at, &plan)
        .unwrap()
    {
        Value::Array(view) => {
            // Literal arrays have established A1 coordinates, independent of the caller.
            // Selecting their second row preserves that origin plus the row offset.
            assert_eq!(view.anchor, Some(CellAddress { row: 1, col: 0 }));
            assert_eq!(view.values, vec![vec![Number(3.), Number(4.)]]);
        }
        _ => panic!("whole row selection must retain its array channel"),
    }
}

#[test]
fn text_forwards_preserve_lossless_text_and_exact_work() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    let at = CellAddress { row: 0, col: 0 };
    for value in [
        Text("abc".into()),
        CellValue::TextUtf16(crate::model::ExcelText::from_units(vec![0xd83d, 0xde00])),
        CellValue::TextUtf16(crate::model::ExcelText::from_units(vec![0xd83d])),
    ] {
        for limit in [0, 2, 100] {
            let budgets = EvaluationBudgets {
                text_bytes: limit,
                ..Default::default()
            };
            let mut baseline = Session::new(
                &workbook,
                Default::default(),
                BudgetMeter::new(budgets).unwrap(),
            );
            let mut session = Session::new(
                &workbook,
                Default::default(),
                BudgetMeter::new(budgets).unwrap(),
            );
            let text = value.text_ref().unwrap();
            let expected = checked_text_view(text, &mut baseline.state.runtime.budget);
            let actual =
                ArgumentCoercion::new(&mut CallHost::new(&mut session, 0, at, ArgumentsRef::Empty))
                    .scalar_text_view(text);
            assert_function_parity(&actual, &expected);
            assert_eq!(
                session.state.runtime.budget.usage,
                baseline.state.runtime.budget.usage
            );
        }
    }
    for text in ["1/2/2020", "invalid", "12:30"] {
        let mut baseline = Session::with_context(&workbook, Default::default());
        let mut session = Session::with_context(&workbook, Default::default());
        let expected = Coercer::new(&mut baseline).coerce_numeric_date_time(text);
        let mut host = CallHost::new(&mut session, 0, at, ArgumentsRef::Empty);

        assert_eq!(
            ContextReads::new(&mut host).date_system_snapshot(),
            workbook.date_system
        );
        assert_eq!(
            ContextReads::new(&mut host).number_locale_snapshot(),
            baseline.policies.number_locale
        );
        let actual = ArgumentCoercion::new(&mut host).legacy_numeric_date_time(text);
        drop(host);
        assert_function_parity(&actual, &expected);
        assert_eq!(
            session.state.runtime.budget.usage,
            baseline.state.runtime.budget.usage
        );
    }
}

#[test]
fn matrix_forward_preserves_values_errors_and_resource_admission() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    let at = CellAddress { row: 0, col: 0 };
    for limit in [0, 1, 4] {
        for value in [
            Value::Array(ArrayView::unknown(vec![vec![Number(2.), Number(3.)]])),
            Value::Scalar(Error(Ref)),
            Value::Scalar(Number(7.)),
            Value::Missing,
        ] {
            let budgets = EvaluationBudgets {
                materialized_values: limit,
                ..EvaluationBudgets::default()
            };
            let mut baseline = Session::new(
                &workbook,
                EvaluationInputs::default(),
                BudgetMeter::new(budgets).unwrap(),
            );
            let mut session = Session::new(
                &workbook,
                EvaluationInputs::default(),
                BudgetMeter::new(budgets).unwrap(),
            );
            let expected = ArgumentReader::new(&mut baseline).matrix(value.clone());
            let actual =
                OperandReader::new(&mut CallHost::new(&mut session, 0, at, ArgumentsRef::Empty))
                    .matrix(value);
            match (actual, expected) {
                (Ok(actual), Ok(expected)) => assert_eq!(actual, expected),
                (Err(EvaluationError::Budget(actual)), Err(EvaluationError::Budget(expected))) => {
                    assert_eq!(actual, expected)
                }
                (actual, expected) => panic!("matrix outcomes differ: {actual:?}, {expected:?}"),
            }
            assert_eq!(
                session.state.runtime.budget.usage,
                baseline.state.runtime.budget.usage
            );
        }
    }
}

#[test]
fn java_control_trim_and_finite_grammar_keep_date_fallback_distinct() {
    assert!(
        matches!(poirot_functions::coercion::compatibility::coerce_numeric_text("\u{1f}1.5\u{0}"),
        CoercionDecision::Number(Ok(value)) if value == 1.5)
    );
    assert!(matches!(
        poirot_functions::coercion::compatibility::coerce_numeric_text("1e999"),
        CoercionDecision::Number(Err(VALUE_ERROR))
    ));
    assert!(matches!(
        poirot_functions::coercion::compatibility::coerce_numeric_text("\u{a0}1\u{a0}"),
        CoercionDecision::DateTime
    ));
    assert!(matches!(
        poirot_functions::coercion::compatibility::coerce_numeric_text("2020/01/02"),
        CoercionDecision::DateTime
    ));
}

const LOOKUP_AT: CellAddress = CellAddress { row: 19, col: 25 };

fn table(session: &mut Session<'_>, source: &str) -> LookupTable {
    let parsed = poirot_parsing::formula::parse_formula(source).unwrap();
    let expression = compile::bound::bind(&parsed);
    LookupTable::admit(
        ExpressionRunner::new(session)
            .expr(0, LOOKUP_AT, &expression)
            .unwrap(),
        ClassicLookupKind::Match,
    )
    .unwrap()
}

#[test]
fn lookup_service_reuses_first_positions_and_preserves_streamed_results() {
    // First duplicates and exact misses follow the existing
    // documented contracts in tests/prepared_lookup_reuse.rs.
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    for (address, value) in [("A1", 1.), ("A2", 2.), ("A3", 2.)] {
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value(address, value)
            .unwrap();
    }
    let mut cached = Session::with_context(&workbook, EvaluationInputs::default());
    cached.state.runtime.lookup_prefixes.enable();
    let mut streamed = Session::with_context(&workbook, EvaluationInputs::default());
    let cached_table = table(&mut cached, "A1:A3");
    let streamed_table = table(&mut streamed, "A1:A3");
    for (key, expected) in [(2., Some(1)), (2., Some(1)), (9., None), (9., None)] {
        let actual = LookupService::new(&mut CallHost::new(
            &mut cached,
            0,
            LOOKUP_AT,
            ArgumentsRef::Empty,
        ))
        .lookup_index(
            &cached_table,
            SearchAxis::Vertical,
            &Number(key),
            SearchMode::Exact,
            ClassicLookupKind::Match,
        );
        let baseline = SessionSearch::new(&mut streamed).classic_lookup_index(
            &streamed_table,
            SearchAxis::Vertical,
            &Number(key),
            SearchMode::Exact,
            ClassicLookupKind::Match,
        );
        assert_eq!(*actual.as_ref().unwrap(), expected);
        assert_function_parity(&actual, &baseline);
    }
    assert_eq!(cached.state.runtime.run_usage.lookup_prefix_scans, 3);
    assert_eq!(cached.state.runtime.run_usage.lookup_prefix_hits, 2);
    assert_eq!(cached.state.runtime.budget.usage.cell_reads, 3);
}

#[test]
fn lookup_service_rejects_empty_geometry_before_candidate_work() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    let table = LookupTable::admit(
        Value::Array(ArrayView::unknown(Vec::new())),
        ClassicLookupKind::Table,
    )
    .unwrap();
    let before = session.state.runtime.budget.usage.clone();
    let result = LookupService::new(&mut CallHost::new(
        &mut session,
        0,
        LOOKUP_AT,
        ArgumentsRef::Empty,
    ))
    .lookup_index(
        &table,
        SearchAxis::Horizontal,
        &Number(2.),
        SearchMode::Exact,
        ClassicLookupKind::Table,
    );
    assert!(matches!(result, Err(FunctionError::Spreadsheet(Ref))));
    assert_eq!(session.state.runtime.budget.usage, before);
}

#[test]
fn lookup_service_preserves_failure_provenance_and_budget_accounting() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    workbook.sheet_mut(0).unwrap().set_value("A2", 2.).unwrap();
    for source in ["A1:A2", "{1;2;3}"] {
        for work in [0, 1, 100] {
            for cached in [false, true] {
                let mut baseline = Session::with_context(&workbook, EvaluationInputs::default());
                let mut actual = Session::with_context(&workbook, EvaluationInputs::default());
                if cached {
                    baseline.state.runtime.lookup_prefixes.enable();
                    actual.state.runtime.lookup_prefixes.enable();
                }
                let baseline_table = table(&mut baseline, source);
                let actual_table = table(&mut actual, source);
                let limits = EvaluationBudgets {
                    run_work: work,
                    ..Default::default()
                };
                baseline.state.runtime.budget = BudgetMeter::new(limits).unwrap();
                actual.state.runtime.budget = BudgetMeter::new(limits).unwrap();
                let target = EvaluationTarget {
                    sheet: 0,
                    address: LOOKUP_AT,
                };
                baseline.state.runtime.budget.push_cell(target);
                actual.state.runtime.budget.push_cell(target);
                let expected = SessionSearch::new(&mut baseline).classic_lookup_index(
                    &baseline_table,
                    SearchAxis::Vertical,
                    &Number(2.),
                    SearchMode::Exact,
                    ClassicLookupKind::Match,
                );
                let result = LookupService::new(&mut CallHost::new(
                    &mut actual,
                    0,
                    LOOKUP_AT,
                    ArgumentsRef::Empty,
                ))
                .lookup_index(
                    &actual_table,
                    SearchAxis::Vertical,
                    &Number(2.),
                    SearchMode::Exact,
                    ClassicLookupKind::Match,
                );
                assert_function_parity(&result, &expected);
                assert_eq!(
                    actual.state.runtime.budget.usage,
                    baseline.state.runtime.budget.usage
                );
                assert_eq!(
                    actual.state.runtime.run_usage,
                    baseline.state.runtime.run_usage
                );
                if work == 0 {
                    assert!(
                        matches!(result,
                            Err(FunctionError::Operational(EvaluationError::Budget(
                                BudgetError { target: Some(failed_at), .. }
                            ))) if failed_at == target
                        ),
                        "{source}, {work}, {cached}: {result:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn reference_intersection_and_relative_resize_have_separate_bounds() {
    let area = Area {
        kind: AreaKind::Area,
        sheet: 2,
        r0: 3,
        r1: 3,
        c0: 4,
        c1: 5,
    };
    let selected = CellReadPlan::implicit(area, CellAddress { row: 90, col: 5 }).unwrap();
    assert_eq!(selected.sheet, 2);
    assert_eq!(selected.address, CellAddress { row: 3, col: 5 });
    assert!(matches!(
        CellReadPlan::implicit(area, CellAddress { row: 3, col: 6 }),
        Err(VALUE_ERROR)
    ));
    let resized = CellReadPlan::relative(area, 2, 3).unwrap();
    assert_eq!(resized.address, CellAddress { row: 5, col: 7 });
    assert!(matches!(
        CellReadPlan::relative(area, MAX_ROWS as usize, 0),
        Err(Ref)
    ));
    assert!(matches!(
        CellReadPlan::relative(area, 0, MAX_COLUMNS as usize),
        Err(Ref)
    ));
}

#[test]
fn scalar_arrays_intersect_rows_columns_and_rectangles() {
    let anchor = CellAddress { row: 3, col: 4 };
    let row = ArrayView::anchored(vec![vec![Blank; 2]], Some(anchor));
    assert_eq!(
        (crate::eval::host::scalar_selection::array_position(
            &row,
            CellAddress { row: 90, col: 5 }
        ))
        .unwrap(),
        Ok((0, 1))
    );
    let column = ArrayView::anchored(vec![vec![Blank]; 2], Some(anchor));
    assert_eq!(
        (crate::eval::host::scalar_selection::array_position(
            &column,
            CellAddress { row: 4, col: 90 }
        ))
        .unwrap(),
        Ok((1, 0))
    );
    let rectangle = ArrayView::anchored(vec![vec![Blank; 2]; 2], Some(anchor));
    assert_eq!(
        (crate::eval::host::scalar_selection::array_position(
            &rectangle,
            CellAddress { row: 4, col: 5 }
        ))
        .unwrap(),
        Ok((1, 1))
    );
    assert_eq!(
        (crate::eval::host::scalar_selection::array_position(
            &rectangle,
            CellAddress { row: 2, col: 5 }
        ))
        .unwrap(),
        Err(VALUE_ERROR)
    );
    assert!(
        crate::eval::host::scalar_selection::array_position(
            &ArrayView::unknown(vec![vec![Blank; 2]]),
            anchor
        )
        .is_err()
    );
    assert_eq!(
        (crate::eval::host::scalar_selection::array_position(
            &ArrayView::unknown(vec![vec![Blank]]),
            anchor
        ))
        .unwrap(),
        Ok((0, 0))
    );
}

/// Compare supported scalar and host failures while keeping dependency causes opaque to equality.
fn assert_function_parity<T: std::fmt::Debug + PartialEq>(
    actual: &Result<T, FunctionError>,
    expected: &Result<T, FunctionError>,
) {
    use crate::eval::limits::EvaluationReasonError;
    match (actual, expected) {
        (Ok(actual), Ok(expected)) => assert_eq!(actual, expected),
        (Err(FunctionError::Spreadsheet(actual)), Err(FunctionError::Spreadsheet(expected))) => {
            assert_eq!(actual, expected)
        }
        (
            Err(FunctionError::Operational(EvaluationError::Budget(actual))),
            Err(FunctionError::Operational(EvaluationError::Budget(expected))),
        ) => assert_eq!(actual, expected),
        (
            Err(FunctionError::Operational(EvaluationError::UnsupportedCapability {
                capability,
                target,
                reason:
                    EvaluationReasonError::UnicodeScalarConsumerCannotProjectIsolatedUtf16Surrogates,
            })),
            Err(FunctionError::Operational(EvaluationError::UnsupportedCapability {
                capability: expected_capability,
                target: expected_target,
                reason:
                    EvaluationReasonError::UnicodeScalarConsumerCannotProjectIsolatedUtf16Surrogates,
            })),
        ) => {
            assert_eq!(capability, expected_capability);
            assert_eq!(target, expected_target);
        }
        (actual, expected) => panic!("function outcomes differ: {actual:?}, {expected:?}"),
    }
}

#[test]
fn small_vector_reservations_keep_representation_and_allocator_causes_distinct() {
    use crate::eval::limits::{EvaluationReasonError, ResourceFailureKind};
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    let mut session = Session::with_context(&book, EvaluationInputs::default());
    let host = CallHost::new(&mut session, 0, AT, ArgumentsRef::Empty);
    let failure =
        host.small_vector_reservation_failure(smallvec::CollectionAllocErr::CapacityOverflow);
    assert!(matches!(failure, EvaluationError::Resource {
        kind: ResourceFailureKind::SizeOverflow,
        target: Some(target),
        reason: EvaluationReasonError::SmallVectorReservation { native },
        ..
    } if target == EvaluationTarget { sheet: 0, address: AT } && matches!(native.as_ref(), smallvec::CollectionAllocErr::CapacityOverflow)));
    let layout = std::alloc::Layout::from_size_align(16, 8).unwrap();
    let failure =
        host.small_vector_reservation_failure(smallvec::CollectionAllocErr::AllocErr { layout });
    assert!(matches!(failure, EvaluationError::Resource {
        kind: ResourceFailureKind::Allocation,
        target: Some(target),
        reason: EvaluationReasonError::SmallVectorReservation { native },
        ..
    } if target == EvaluationTarget { sheet: 0, address: AT } && matches!(native.as_ref(), smallvec::CollectionAllocErr::AllocErr { layout: retained } if *retained == layout)));
}
