use super::super::rectangular_traversal::RectangularTraversal;
use super::super::shape::Shape;
use crate::eval::config::EvaluationInputs;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{BudgetMeter, CapabilityId, EvaluationBudgets, EvaluationFailure};
use crate::eval::session::Session;
use crate::model::{CellValue, Workbook};

#[test]
fn rectangular_capacity_overflow_is_an_attributed_resource_failure() {
    let workbook = Workbook::new();
    let target = EvaluationTarget {
        sheet: 0,
        address: "B2".parse().unwrap(),
    };
    // Each capacity exceeds Vec's byte limit, without requesting real heap storage.
    let shapes = [
        Shape {
            rows: usize::MAX / std::mem::size_of::<Vec<CellValue>>(),
            columns: 1,
        },
        Shape {
            rows: 1,
            columns: usize::MAX / std::mem::size_of::<CellValue>(),
        },
    ];
    for shape in shapes {
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.state.runtime.stack.consumer = Some(target);
        session.state.runtime.budget = BudgetMeter::new(EvaluationBudgets {
            run_work: u64::MAX,
            cell_work: u64::MAX,
            materialized_values: usize::MAX,
            ..Default::default()
        })
        .unwrap();
        let failure = RectangularTraversal::new(&mut session)
            .array(target.address, shape, |_, _, _| {
                panic!("reservation must reject before evaluating any member")
            })
            .err()
            .unwrap();
        assert!(matches!(failure, EvaluationFailure::Resource {
            kind: crate::eval::limits::ResourceFailureKind::SizeOverflow,
            capability: CapabilityId::FunctionAdapter,
            source: Some(source), ..
        } if source == target));
        assert_eq!(
            session.state.runtime.budget.usage.materialized_values,
            (shape.rows * shape.columns) as u64
        );
    }
}
