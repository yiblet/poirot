//! Original proofs of Microsoft's full and resizing spill-reference contract:
//! https://support.microsoft.com/en-us/excel/spilled-range-operator
//! Error/no-footprint and one-cell identity cases below are explicit Poirot policies.
//! Stored ANCHORARRAY spelling follows the pinned rust_xlsxwriter 0.101.0 author docs:
//! https://docs.rs/rust_xlsxwriter/0.101.0/rust_xlsxwriter/struct.Formula.html
//! No external implementation, tests or fixture data were copied or adapted.
use poirot::eval::overlay::{Evaluation, EvaluationContext, Overlay};
use poirot::eval::{BudgetKind, EvaluationFailure, EvaluationTarget};
use poirot::model::{CellError, CellValue, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
fn book(formulas: &[(&str, &str)]) -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for &(address, formula) in formulas {
        sheet.set_formula(at(address).address, formula).unwrap();
    }
    book
}
fn verify(book: &Workbook, expected: &[(&str, CellValue)]) {
    for reverse in [false, true] {
        let mut direct = Evaluation::with_workbook(book)
            .build(Default::default())
            .unwrap();
        let mut overlay = Overlay::with_workbook(book).build().unwrap();
        let context = EvaluationContext::default();
        let mut expected = expected.to_vec();
        if reverse {
            expected.reverse();
        }
        for _ in 0..2 {
            let mut evaluation = overlay.evaluation(&context).unwrap();
            for (address, value) in &expected {
                assert_eq!(
                    direct.cell(at(address)).unwrap(),
                    *value,
                    "direct {address}"
                );
                assert_eq!(
                    evaluation.cell(at(address)).unwrap(),
                    *value,
                    "overlay {address}"
                );
            }
        }
    }
}
#[test]
fn full_reference_scalar_consumers_and_forwarding_roots() {
    let book = book(&[
        ("A1", "{1,2;3,4}"),
        ("D1", "A1#"),
        ("G1", "D1#"),
        ("J1", "SUM(A1#)"),
        ("J2", "ROWS(A1#)"),
        ("J3", "COLUMNS(A1#)"),
        ("J4", "SUM(_xlfn.ANCHORARRAY(A1))"),
        ("J5", "SUM(ANCHORARRAY(A1))"),
        ("J6", "@A1#"),
        ("J7", "SUM(A1#:B2)"),
    ]);
    verify(
        &book,
        &[
            ("E2", 4.0.into()),
            ("H2", 4.0.into()),
            ("G1", 1.0.into()),
            ("J1", 10.0.into()),
            ("J2", 2.0.into()),
            ("J3", 2.0.into()),
            ("J4", 10.0.into()),
            ("J5", 10.0.into()),
            ("J6", CellValue::Error(CellError::Value)),
            ("J7", 10.0.into()),
        ],
    );
}
#[test]
fn one_cell_arrays_and_forwarded_references_retain_identity() {
    let book = book(&[
        ("A1", "{7}"),
        ("A3", "7"),
        ("C1", "A1#"),
        ("E1", "C1#"),
        ("G1", "SUM(E1#)"),
        ("G2", "SUM(A3#)"),
        ("G3", "A1"),
        ("G4", "SUM(G3#)"),
        ("G5", "SUM(A1#)"),
        ("G6", "SUM(A1##)"),
    ]);
    verify(
        &book,
        &[
            ("G1", 7.0.into()),
            ("G2", CellValue::Error(CellError::Ref)),
            ("G3", 7.0.into()),
            ("G4", CellValue::Error(CellError::Ref)),
            ("G5", 7.0.into()),
            ("G6", 7.0.into()),
        ],
    );
}
#[test]
fn ordinary_area_roots_materialize_but_cell_roots_remain_scalar() {
    let mut book = book(&[
        ("D1", "A1:A2"),
        ("G1", "SUM(D1#)"),
        ("D4", "A1"),
        ("G4", "SUM(D4#)"),
    ]);
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_value(at("A1").address, 2.).unwrap();
    sheet.set_value(at("A2").address, 3.).unwrap();
    verify(
        &book,
        &[
            ("D2", 3.0.into()),
            ("G1", 5.0.into()),
            ("D4", 2.0.into()),
            ("G4", CellValue::Error(CellError::Ref)),
        ],
    );
}
#[test]
fn invalid_anchor_shapes_absent_cells_children_and_errors_follow_declared_policy() {
    let book = book(&[
        ("A1", "{1,2}"),
        ("D1", "SUM(B1#)"),
        ("D2", "SUM(Z99#)"),
        ("D3", "SUM((A1:B2)#)"),
        ("D4", "SUM((A1,A2)#)"),
        ("D5", "SUM((1)#)"),
        ("D6", "SUM(NoSheet!A1#)"),
        ("D7", "SUM((#N/A)#)"),
        ("D8", "ANCHORARRAY()"),
    ]);
    verify(
        &book,
        &[
            ("D1", CellValue::Error(CellError::Ref)),
            ("D2", CellValue::Error(CellError::Ref)),
            ("D3", CellValue::Error(CellError::Ref)),
            ("D4", CellValue::Error(CellError::Ref)),
            ("D5", CellValue::Error(CellError::Ref)),
            ("D6", CellValue::Error(CellError::Ref)),
            ("D7", CellValue::Error(CellError::NA)),
            ("D8", CellValue::Error(CellError::Value)),
        ],
    );
}
#[test]
fn grow_shrink_block_remove_and_restore_invalidate_retained_references() {
    let book = book(&[
        ("A1", "IF(J1,{1,2;3,4},{9})"),
        ("D1", "A1#"),
        ("G1", "SUM(D1#)"),
    ]);
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let context = EvaluationContext::default();
    for (input, sum, last, old_child) in [
        (false, 9., "D1", CellValue::Blank),
        (true, 10., "E2", 4.0.into()),
        (false, 9., "D1", CellValue::Blank),
    ] {
        overlay.set_value(at("J1"), input).unwrap();
        let mut e = overlay.evaluation(&context).unwrap();
        assert_eq!(e.cell(at("G1")).unwrap(), sum.into());
        assert_eq!(e.cell(at("E2")).unwrap(), old_child);
        drop(e);
        let mut materialized = overlay.workbook().clone();
        materialized.apply(overlay.changes()).unwrap();
        assert_eq!(
            materialized
                .sheet(0)
                .unwrap()
                .complete_spill_extent(at("D1").address),
            Some(at(last).address)
        );
    }
    overlay.set_value(at("B1"), 99.).unwrap();
    overlay.set_value(at("J1"), true).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("G1"))
            .unwrap(),
        CellValue::Error(CellError::Spill)
    );
    overlay.remove_cell(at("B1")).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("G1"))
            .unwrap(),
        10.0.into()
    );
    overlay.remove_cell(at("A1")).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("G1"))
            .unwrap(),
        CellValue::Error(CellError::Ref)
    );
}
#[test]
fn quoted_cross_sheet_reference_prepares_the_actual_supplier_only() {
    let mut book = book(&[("A1", "'Spill sheet'!$A$1#"), ("G1", "SUM(A1#)")]);
    let other = book.add_sheet("Spill sheet").unwrap();
    other.set_formula(at("A1").address, "{2;3}").unwrap();
    other.set_formula(at("D1").address, "{7,8}").unwrap();
    let mut e = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(e.cell(at("G1")).unwrap(), 5.0.into());
    assert_eq!(
        e.cached_value(EvaluationTarget {
            sheet: 1,
            address: at("E1").address
        }),
        None
    );
    assert_eq!(e.cell(at("A2")).unwrap(), 3.0.into());
    assert_eq!(
        e.cell(EvaluationTarget {
            sheet: 1,
            address: at("E1").address
        })
        .unwrap(),
        8.0.into()
    );
}
#[test]
fn snapshot_retains_complete_footprints_and_allows_source_overrides() {
    let book = book(&[
        ("A1", "{1,2}"),
        ("D1", "A1#"),
        ("G1", "SUM(ANCHORARRAY(D1))"),
    ]);
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("G1"))
            .unwrap(),
        3.0.into()
    );
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (book, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach(book).unwrap();
    let mut e = restored.evaluation(&context).unwrap();
    assert_eq!(e.cell(at("E1")).unwrap(), 2.0.into());
    assert_eq!(e.cell(at("G1")).unwrap(), 3.0.into());
    assert_eq!(e.usage().formula_executions, 0);
    drop(e);
    restored.set_formula(at("A1"), "{7}").unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("G1"))
            .unwrap(),
        7.0.into()
    );
}
#[test]
fn direct_changes_and_xlsx_reopen_preserve_singleton_and_rectangular_extents() {
    let mut book = book(&[
        ("A1", "{1,2;3,4}"),
        ("D1", "A1#"),
        ("G1", "{9}"),
        ("J1", "SUM(G1#)"),
    ]);
    let mut e = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(e.cell(at("J1")).unwrap(), 9.0.into());
    let changes = e.changes();
    drop(e);
    book.apply(changes).unwrap();
    for (anchor, last) in [("A1", "B2"), ("D1", "E2"), ("G1", "G1")] {
        assert_eq!(
            book.sheet(0)
                .unwrap()
                .complete_spill_extent(at(anchor).address),
            Some(at(last).address)
        );
    }
    let bytes = poirot_io::write_xlsx_to_buffer(&book).unwrap();
    let reopened = poirot_io::read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    for (anchor, last) in [("A1", "B2"), ("D1", "E2"), ("G1", "G1")] {
        assert_eq!(
            reopened
                .sheet(0)
                .unwrap()
                .complete_spill_extent(at(anchor).address),
            Some(at(last).address)
        );
    }
    assert_eq!(
        reopened
            .sheet(0)
            .unwrap()
            .get(at("D1").address)
            .unwrap()
            .formula
            .as_deref(),
        Some("_xlfn.ANCHORARRAY(A1)")
    );
    verify(&reopened, &[("E2", 4.0.into()), ("J1", 9.0.into())]);
}
#[test]
fn full_column_reference_root_obeys_materialization_budget_atomically() {
    let book = book(&[("D1", "A:A")]);
    let mut context = EvaluationContext::default();
    context.limits = context.limits.max_collection_values(100);
    let mut e = Evaluation::with_workbook(&book).build(context).unwrap();
    assert!(
        matches!(e.cell(at("D1")),Err(EvaluationFailure::Budget(f)) if f.kind==BudgetKind::Materialization)
    );
    assert!(e.changes().is_empty());
    assert_eq!(e.values().count(), 0);
}

#[test]
fn error_members_do_not_destroy_accepted_reference_geometry() {
    let book = book(&[
        ("A1", "{#N/A,2}"),
        ("D1", "ROWS(A1#)"),
        ("D2", "COLUMNS(A1#)"),
        ("D3", "INDEX(A1#,1,2)"),
        ("D4", "SUM(A1#)"),
        ("G1", "{#N/A}"),
        ("J1", "ROWS(G1#)"),
        ("J2", "COLUMNS(G1#)"),
        ("J3", "SUM(G1#)"),
    ]);
    verify(
        &book,
        &[
            ("D1", 1.0.into()),
            ("D2", 2.0.into()),
            ("D3", 2.0.into()),
            ("D4", CellValue::Error(CellError::NA)),
            ("J1", 1.0.into()),
            ("J2", 1.0.into()),
            ("J3", CellValue::Error(CellError::NA)),
        ],
    );
}

#[test]
fn spill_postfix_precedes_range_and_expands_the_complete_envelope() {
    let book = book(&[
        ("B2", "{2,3;4,5}"),
        ("F1", "SUM(A1:B2#)"),
        ("F2", "ROWS(A1:B2#)"),
        ("F3", "COLUMNS(A1:B2#)"),
    ]);
    verify(
        &book,
        &[("F1", 14.0.into()), ("F2", 3.0.into()), ("F3", 3.0.into())],
    );
}

#[test]
fn overlay_flush_preserves_singleton_extent_and_authored_child_style() {
    let mut book = book(&[("A1", "{1,2}"), ("D1", "SUM(A1#)")]);
    book.sheet_mut(0)
        .unwrap()
        .set(
            at("B1").address,
            poirot::model::Cell {
                shared_formula: None,
                value: CellValue::Blank,
                formula: None,
                style: poirot::model::CellStyle {
                    number_format: Some("0.00".into()),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let context = EvaluationContext::default();
    let changes;
    {
        let mut overlay = Overlay::with_workbook(&mut book).build().unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("D1"))
                .unwrap(),
            3.0.into()
        );
        overlay.flush().unwrap();
        overlay.set_formula(at("A1"), "{8}").unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("D1"))
                .unwrap(),
            8.0.into()
        );
        overlay.flush().unwrap();
        // Flush publishes authored edits; change export separately publishes caches.
        changes = overlay.changes();
    }
    book.apply(changes).unwrap();
    let sheet = book.sheet(0).unwrap();
    assert_eq!(
        sheet.complete_spill_extent(at("A1").address),
        Some(at("A1").address)
    );
    assert_eq!(sheet.get(at("B1").address).unwrap().value, CellValue::Blank);
    assert_eq!(
        sheet
            .get(at("B1").address)
            .unwrap()
            .style
            .number_format
            .as_deref(),
        Some("0.00")
    );
}

#[test]
fn signed_single_area_references_share_anchor_resolution_and_root_publication() {
    let book = book(&[
        ("A1", "{1,2;3,4}"),
        ("D1", "SUM(OFFSET(A1,0,0)#)"),
        ("D2", "SUM(OFFSET(A1,0,0,1,1)#)"),
        ("D3", "SUM(OFFSET(A1,-1,0)#)"),
        ("G1", "OFFSET(A1,0,0,2,2)"),
        ("J1", "SUM(G1#)"),
    ]);
    verify(
        &book,
        &[
            ("D1", 10.0.into()),
            ("D2", 10.0.into()),
            ("D3", CellValue::Error(CellError::Ref)),
            ("H2", 4.0.into()),
            ("J1", 10.0.into()),
        ],
    );
}
#[test]
fn unpublished_spill_reference_cycles_fail_without_partial_publication() {
    for formulas in [vec![("A1", "A1#")], vec![("A1", "D1#"), ("D1", "A1#")]] {
        let book = book(&formulas);
        let mut direct = Evaluation::with_workbook(&book)
            .build(Default::default())
            .unwrap();
        assert!(direct.cell(at("A1")).is_err());
        assert!(direct.changes().is_empty());
        assert_eq!(direct.values().count(), 0);
        let mut overlay = Overlay::with_workbook(&book).build().unwrap();
        let context = EvaluationContext::default();
        let mut e = overlay.evaluation(&context).unwrap();
        assert!(e.cell(at("A1")).is_err());
        assert!(e.result_changes().unwrap().is_empty());
        assert_eq!(e.values().count(), 0);
    }
}

#[test]
fn singleton_array_operations_preserve_identity_until_the_consumer_selects_a_scalar() {
    let book = book(&[
        ("A1", "{7}+0"),
        ("A2", "-{7}"),
        ("A3", "IF({TRUE},7,8)"),
        ("D1", "SUM(A1#)"),
        ("D2", "SUM(A2#)"),
        ("D3", "SUM(A3#)"),
        ("G1", "SUM({7})"),
        ("J1", "SUM(G1#)"),
    ]);
    verify(
        &book,
        &[
            ("D1", 7.0.into()),
            ("D2", (-7.0).into()),
            ("D3", 7.0.into()),
            ("J1", CellValue::Error(CellError::Ref)),
        ],
    );
}

#[test]
fn scalar_intersection_publication_still_selects_one_value_from_array_channels() {
    let book = book(&[
        ("A1", "{7}+0"),
        ("A2", "-{7}"),
        ("A3", "IF({TRUE},7,8)"),
        ("D1", "{1,2;3,4}"),
        ("G1", "SUM(D1#)"),
    ]);
    let policy = poirot::eval::overlay::EvaluationPolicy {
        arrays: poirot::eval::ArrayResultPolicy::ScalarIntersection,
        ..Default::default()
    };
    let mut direct = Evaluation::with_workbook(&book)
        .with_policy(policy.clone())
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy)
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let mut e = overlay.evaluation(&context).unwrap();
    for (address, value) in [
        ("A1", 7.0.into()),
        ("A2", (-7.0).into()),
        ("A3", 7.0.into()),
        ("D1", 1.0.into()),
        ("E1", CellValue::Blank),
        ("G1", CellValue::Error(CellError::Ref)),
    ] {
        assert_eq!(direct.cell(at(address)).unwrap(), value);
        assert_eq!(e.cell(at(address)).unwrap(), value);
    }
}
