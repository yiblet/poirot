use super::super::dependencies::Dependencies;
use super::super::validity::Validity;
use super::super::value_read_order::ValueReadOrder;
use crate::eval::coordinates::{CellMap, CellSet};
use crate::model::CellAddress;
use crate::model::query::RangeMembership;

#[test]
fn compressed_read_spans_preserve_first_occurrences_across_repeated_traversals() {
    let key = |sheet, row, col| (sheet, CellAddress { row, col });
    let sequence = [
        key(0, 5, 4),
        key(0, 6, 4),
        key(0, 7, 4),
        key(0, 6, 4),
        key(0, 0, 2),
        key(0, 0, 3),
        key(0, 0, 4),
        key(0, 5, 4),
        key(1, 0, 0),
        key(1, 1, 0),
        key(0, 0, 2),
        key(0, 3, 9),
    ];
    for mask in 1usize..(1 << sequence.len()) {
        let mut dependencies = Dependencies::default();
        for point in sequence {
            dependencies.value_read(point);
        }
        let mut causes = CellMap::default();
        for (position, point) in sequence.iter().enumerate() {
            if mask & (1 << position) != 0 {
                causes.insert(*point, 1);
            }
        }
        let mut seen = CellSet::default();
        let expected = sequence
            .into_iter()
            .filter(|point| causes.contains_key(point) && seen.insert(*point))
            .collect::<Vec<_>>();
        let mut budget = crate::eval::limits::BudgetMeter::new(
            crate::eval::limits::EvaluationBudgets::default(),
        )
        .unwrap();
        let causes = Validity::Causes(causes);
        let actual = dependencies
            .ordered_causes(&causes, &mut budget)
            .unwrap()
            .unwrap();
        let mut actual = actual;
        let mut observed = Vec::new();
        while let Some((point, _)) = actual.next(&causes, &mut budget).unwrap() {
            observed.push(point);
        }
        let actual = observed;
        assert_eq!(actual, expected);
    }
}

#[test]
fn one_span_yields_causes_lazily_and_stops_after_last_dirty_coordinate() {
    let first = (0, CellAddress { row: 0, col: 0 });
    let second = (0, CellAddress { row: 1, col: 0 });
    let dependencies = Dependencies {
        value_reads: ValueReadOrder::One(RangeMembership {
            sheet: 0,
            first: first.1,
            last: CellAddress {
                row: 1_000_000,
                col: 0,
            },
        }),
        ..Default::default()
    };
    let causes = Validity::Causes([(first, 7), (second, 9)].into_iter().collect());
    let mut budget =
        crate::eval::limits::BudgetMeter::new(crate::eval::limits::EvaluationBudgets {
            run_work: 4,
            ..Default::default()
        })
        .unwrap();
    let mut ordered = dependencies
        .ordered_causes(&causes, &mut budget)
        .unwrap()
        .unwrap();
    assert_eq!(
        ordered.next(&causes, &mut budget).unwrap(),
        Some((first, 7))
    );
    assert_eq!(
        ordered.next(&causes, &mut budget).unwrap(),
        Some((second, 9))
    );
    assert_eq!(ordered.next(&causes, &mut budget).unwrap(), None);
}

#[test]
fn metadata_and_membership_without_value_evidence_do_not_certify_multiple_causes() {
    let first = (0, CellAddress { row: 0, col: 0 });
    let second = (0, CellAddress { row: 1, col: 0 });
    let mut dependencies = Dependencies::default();
    dependencies.value_read(first);
    let causes = Validity::Causes([(first, 1), (second, 2)].into_iter().collect());
    let mut budget =
        crate::eval::limits::BudgetMeter::new(crate::eval::limits::EvaluationBudgets::default())
            .unwrap();
    assert!(
        dependencies
            .ordered_causes(&causes, &mut budget)
            .unwrap()
            .is_none()
    );
    dependencies.value_read(second);
    dependencies.metadata_reads = true;
    assert!(
        dependencies
            .ordered_causes(&causes, &mut budget)
            .unwrap()
            .is_none()
    );
    let cause = Validity::Cause {
        key: first,
        version: 1,
    };
    assert!(
        dependencies
            .ordered_causes(&cause, &mut budget)
            .unwrap()
            .is_none()
    );
}
