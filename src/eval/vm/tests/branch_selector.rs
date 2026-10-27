use super::super::branch_selector::BranchSelector;
use super::super::selection::Selection;
use super::super::shape::Shape;
use crate::eval::config::EvaluationInputs;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{BudgetMeter, CapabilityId, EvaluationBudgets, EvaluationError};
use crate::eval::session::Session;
use crate::model::Workbook;

#[test]
fn selection_capacity_overflow_is_an_attributed_resource_failure() {
    let workbook = Workbook::new();
    let target = EvaluationTarget {
        sheet: 0,
        address: "B2".parse().unwrap(),
    };
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.state.runtime.stack.consumer = Some(target);
    session.state.runtime.budget = BudgetMeter::new(EvaluationBudgets {
        run_work: u64::MAX,
        cell_work: u64::MAX,
        materialized_values: usize::MAX,
        ..Default::default()
    })
    .unwrap();
    let count = usize::MAX / std::mem::size_of::<Selection>();
    let result = BranchSelector::new(&mut session).expand(
        vec![Selection::Pending],
        Shape {
            rows: 1,
            columns: 1,
        },
        Shape {
            rows: count,
            columns: 1,
        },
    );
    assert!(matches!(result, Err(EvaluationError::Resource {
        kind: crate::eval::limits::ResourceFailureKind::SizeOverflow,
        capability: CapabilityId::FunctionAdapter,
        target: Some(source), ..
    }) if source == target));
    assert_eq!(
        session.state.runtime.budget.usage.materialized_values,
        count as u64
    );
}
