//! Cell lifecycle tests: completion delivery, depth restoration and spill-aware source resolution.

use super::super::composition::Session;
use super::cell_attempt::CellAttempt;
use super::cell_evaluator::CellEvaluator;
use super::cell_read::CellRead;
use super::completion_context::CompletionContext;
use super::resolved_source::ResolvedSource;
use crate::eval::config::{ArrayResultPolicy, EvaluationInputs};
use crate::eval::coordinates::{CellSet, EvaluationTarget};
use crate::eval::limits::{
    BudgetMeter, CapabilityId, EvalResult, EvaluationBudgets, EvaluationError,
};
use crate::eval::ports::{CellCompletion, CellObserver, CellProvenance};
use crate::eval::{session, spill};
use crate::model::CellError::Ref;
use crate::model::CellValue::{Blank, Error, Number};
use crate::model::{CellAddress, MAX_ROWS, Workbook};

/// Reject every notification to test completion without attaching an execution session.
struct RejectingObserver {
    observed_failures: Vec<bool>,
}

impl CellObserver for RejectingObserver {
    fn completed(&mut self, completion: CellCompletion<'_>) -> EvalResult<()> {
        self.observed_failures.push(completion.result.is_err());
        Err(EvaluationError::StalePlan)
    }
}

#[test]
fn completion_ports_notify_aborts_and_preserve_the_original_failure() {
    let target = EvaluationTarget {
        sheet: 0,
        address: "A1".parse().unwrap(),
    };
    let original = EvaluationError::ResourceLimit {
        capability: CapabilityId::FormulaParsing,
        target: Some(target),
        reason: crate::eval::limits::EvaluationReasonError::ObserverRejected,
    };
    let mut preparation = spill::SpillPreparation::default();
    let mut budget = BudgetMeter::new(EvaluationBudgets::default()).unwrap();
    let visiting = CellSet::default();
    let mut reads = None;
    let mut observer = RejectingObserver {
        observed_failures: Vec::new(),
    };
    for (result, expected) in [
        (Err(original.clone()), original),
        (Ok(Number(6.)), EvaluationError::StalePlan),
    ] {
        let read = CellRead {
            attempt: CellAttempt {
                target,
                consumer: None,
                provenance: CellProvenance::ExternalCache,
                completion_candidate: true,
            },
            result,
        };
        let mut port: Option<&mut dyn CellObserver> = Some(&mut observer);
        let actual = read
            .complete(CompletionContext {
                array_result_policy: ArrayResultPolicy::ScalarIntersection,
                settling_spills: false,
                preparation: &mut preparation,
                budget: &mut budget,
                visiting: &visiting,
                reads: &mut reads,
                consumer: None,
                observer: &mut port,
            })
            .unwrap_err();
        match (actual, expected) {
            (EvaluationError::StalePlan, EvaluationError::StalePlan) => {}
            (
                EvaluationError::ResourceLimit {
                    capability,
                    target: actual,
                    reason: crate::eval::limits::EvaluationReasonError::ObserverRejected,
                },
                EvaluationError::ResourceLimit {
                    capability: expected,
                    target,
                    reason: crate::eval::limits::EvaluationReasonError::ObserverRejected,
                },
            ) => {
                assert_eq!(capability, expected);
                assert_eq!(actual, target);
            }
            (actual, expected) => {
                panic!("completion changed original failure: {actual:?}, {expected:?}")
            }
        }
    }
    assert_eq!(observer.observed_failures, [true, false]);
}

#[test]
fn recursive_depth_failure_restores_caller_attribution_and_expression_demand() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    let output = "A1".parse().unwrap();
    let dependency = "B1".parse::<crate::model::CellAddress>().unwrap();
    let literal = "C1".parse().unwrap();
    sheet.set_formula(output, "B1+1").unwrap();
    sheet.set_value(dependency, 7.).unwrap();
    sheet.set_value(literal, 11.).unwrap();
    let budget = BudgetMeter::new(EvaluationBudgets {
        depth: 1,
        ..Default::default()
    })
    .unwrap();
    let mut session = Session::new(&workbook, EvaluationInputs::default(), budget);
    let caller = EvaluationTarget {
        sheet: 0,
        address: "D1".parse().unwrap(),
    };
    session.state.runtime.stack.consumer = Some(caller);
    session.state.expression_demand = session::ExpressionDemand::Array;
    assert!(CellEvaluator::new(&mut session).cell(0, output).is_err());
    assert_eq!(session.state.runtime.stack.consumer, Some(caller));
    assert!(session.state.expression_demand == session::ExpressionDemand::Array);
    assert!(session.state.runtime.stack.visiting.is_empty());
    // This succeeds under the same depth-one meter only if the failed frame was left.
    assert_eq!(
        CellEvaluator::new(&mut session).cell(0, literal).unwrap(),
        Number(11.)
    );
    assert_eq!(session.state.runtime.stack.consumer, Some(caller));
    assert!(session.state.expression_demand == session::ExpressionDemand::Array);
}

#[test]
fn source_resolution_keeps_formula_blanks_distinct_from_incoming_spill_coordinates() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet.set_formula("A1", "{1,2}").unwrap();
    sheet
        .set_spill_cached_value(
            "B1".parse().unwrap(),
            "A1".parse().unwrap(),
            "{1,2}",
            Number(99.),
        )
        .unwrap();
    sheet.set_formula("D1", "\"\"").unwrap();
    sheet.set_value("E1", Blank).unwrap();
    sheet.set_value("F1", 0.).unwrap();
    let inputs = crate::eval::inputs::RunInputs::new(
        &workbook,
        [(0, "B1".parse().unwrap(), Number(7.))],
        1,
        std::sync::Arc::new(()),
    )
    .unwrap();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.sources.inputs = Some(&inputs);
    session.policies.array_result_policy = ArrayResultPolicy::DynamicSpill;
    for (address, accepts) in [
        ("A1", false),
        ("B1", true),
        ("D1", false),
        ("E1", true),
        ("F1", false),
        ("G1", true),
    ] {
        assert_eq!(
            ResolvedSource::resolve(&session, 0, address.parse().unwrap())
                .unwrap()
                .accepts_spill,
            accepts,
            "{address}"
        );
    }
    assert_eq!(
        ResolvedSource::resolve(&session, 0, "B1".parse().unwrap())
            .unwrap()
            .cell
            .unwrap()
            .value,
        Number(7.)
    );
    assert!(ResolvedSource::resolve(&session, 1, "A1".parse().unwrap()).is_none());
    assert!(
        ResolvedSource::resolve(
            &session,
            0,
            CellAddress {
                row: MAX_ROWS,
                col: 0
            }
        )
        .is_none()
    );
}

#[test]
fn overlay_replaces_a_formula_cycle_with_a_literal() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    let a = "A1".parse().unwrap();
    let b = "B1".parse().unwrap();
    sheet.set_formula(a, "B1").unwrap();
    sheet.set_formula(b, "A1").unwrap();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    assert_eq!(
        CellEvaluator::new(&mut session).cell(0, a).unwrap(),
        Error(Ref)
    );
    assert!(session.state.runtime.stack.visiting.is_empty());
    let inputs = crate::eval::inputs::RunInputs::new(
        &workbook,
        [(0, b, Number(7.))],
        1,
        std::sync::Arc::new(()),
    )
    .unwrap();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.sources.inputs = Some(&inputs);
    assert_eq!(
        CellEvaluator::new(&mut session).cell(0, a).unwrap(),
        Number(7.)
    );
    assert_eq!(session.state.runtime.run_usage.formula_executions, 1);
    assert_eq!(
        session.state.runtime.memo.cache.get(&(0, b)),
        Some(&Number(7.))
    );
    assert!(session.state.runtime.stack.visiting.is_empty());
}

#[test]
fn dynamic_spill_keeps_blank_footprint_detection_and_child_discovery() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    let a = "A1".parse().unwrap();
    let b = "B1".parse().unwrap();
    sheet.set_formula(a, "IF(B1=0,{1,2},{3,4})").unwrap();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.policies.array_result_policy = ArrayResultPolicy::DynamicSpill;
    assert!(
        matches!(CellEvaluator::new(&mut session).cell(0, a), Err(EvaluationError::UnsupportedCapability { reason, .. }) if matches!(reason, crate::eval::limits::EvaluationReasonError::ArrayResultReadsItsOwnUnpublishedFootprint))
    );
    assert!(!session.state.runtime.memo.cache.contains_key(&(0, a)));
    assert!(session.state.runtime.stack.visiting.is_empty());

    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(a, "{1,2}")
        .unwrap();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.policies.array_result_policy = ArrayResultPolicy::DynamicSpill;
    assert_eq!(
        CellEvaluator::new(&mut session).cell(0, b).unwrap(),
        Number(2.)
    );
    assert_eq!(
        session.state.runtime.memo.cache.get(&(0, a)),
        Some(&Number(1.))
    );
    assert_eq!(
        session
            .state
            .runtime
            .preparation
            .admitted_role((0, b))
            .owner(b),
        Some(a)
    );
    assert!(session.state.runtime.stack.visiting.is_empty());
}
