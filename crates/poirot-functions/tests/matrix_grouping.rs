//! Independent membership and hierarchy examples; no external fixture material.
use poirot_functions::matrix::*;
fn budget() -> Budget {
    Budget {
        max_cells: 1_000_000,
        max_work: 1_000_000,
    }
}
#[test]
fn partition_and_hierarchy_keep_original_members_and_parentage() {
    let keys = Matrix::new(4, 2, vec![2, 2, 1, 2, 2, 1, 1, 1]).unwrap();
    let partition = partition(&keys, Axis::Rows, i32::cmp, budget()).unwrap();
    assert_eq!(partition.order, vec![3, 1, 2, 0]);
    let groups = hierarchy(&keys, &partition, i32::cmp, budget()).unwrap();
    assert_eq!(groups.len(), 7);
    assert_eq!(groups[0].members, 0..4);
    assert_eq!(groups[1].members, 0..2);
    assert_eq!(groups[4].members, 2..4);
    assert_eq!(
        group_layout(
            &groups,
            2,
            FieldRelationship::Hierarchy,
            2,
            |a, b| a.cmp(&b),
            budget()
        )
        .unwrap(),
        vec![2, 3, 1, 5, 6, 4, 0]
    );
    assert_eq!(
        group_layout(
            &groups,
            2,
            FieldRelationship::Hierarchy,
            -2,
            |a, b| a.cmp(&b),
            budget()
        )
        .unwrap(),
        vec![0, 1, 2, 3, 4, 5, 6]
    );
}
#[test]
fn partition_comparisons_are_subquadratic_and_malformed_layout_is_fallible() {
    let keys = Matrix::new(2048, 1, (0..2048).rev().collect::<Vec<i32>>()).unwrap();
    let count = std::cell::Cell::new(0);
    let partition = partition(
        &keys,
        Axis::Rows,
        |a, b| {
            count.set(count.get() + 1);
            a.cmp(b)
        },
        budget(),
    )
    .unwrap();
    assert_eq!(partition.groups.len(), 2048);
    assert!(count.get() < 2048 * 32);
    let malformed = [
        Group {
            depth: 0,
            parent: None,
            members: 0..1,
        },
        Group {
            depth: 1,
            parent: Some(9),
            members: 0..1,
        },
    ];
    assert!(
        group_layout(
            &malformed,
            1,
            FieldRelationship::Hierarchy,
            0,
            |a, b| a.cmp(&b),
            budget()
        )
        .is_err()
    );
    assert!(
        hierarchy(
            &keys,
            &partition,
            i32::cmp,
            Budget {
                max_cells: 10,
                max_work: 10
            }
        )
        .is_err()
    );
}
