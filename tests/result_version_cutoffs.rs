//! Incremental reuse follows observable values without publishing stale evidence.
use poirot::calculation::{EvaluationContext, EvaluationTarget};
use poirot::editing::WorkbookChange;
use poirot::eval::overlay::Overlay;
use poirot::styles::CellStyle;
use poirot::{CellValue, Workbook};
use std::time::{Duration, UNIX_EPOCH};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
fn book(inputs: &[(&str, f64)], formulas: &[(&str, &str)]) -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for &(cell, value) in inputs {
        sheet.set_value(at(cell).address, value).unwrap();
    }
    for &(cell, formula) in formulas {
        sheet.set_formula(at(cell).address, formula).unwrap();
    }
    book
}

#[test]
fn stable_intermediate_skips_expensive_consumers_and_keeps_original_dependencies() {
    let source = book(
        &[("A1", 1.1), ("D1", 10.), ("D2", 20.)],
        &[("B1", "ROUND(A1,0)"), ("C1", "B1+SUM(D1:D3)")],
    );
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(31.)
    );
    overlay.set_value(at("A1"), 1.2).unwrap();
    assert_eq!(overlay.cached_cells(), 0);
    assert!(overlay.cached_value(at("C1")).is_none());
    assert_eq!(overlay.retained_usage().cells, 2);
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("C1")).unwrap(), CellValue::Number(31.));
    assert_eq!(next.usage().formula_executions, 1);
    drop(next);
    // Reusing C1 must preserve its clean range as well as its dirty B1 cause.
    overlay.set_value(at("D3"), 5.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(36.)
    );
    overlay.set_value(at("A1"), 2.2).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(37.)
    );
}

#[test]
fn all_changed_paths_must_settle_before_reusing_a_consumer() {
    let source = book(
        &[("A1", 1.1), ("A2", 2.)],
        &[("B1", "ROUND(A1,0)"), ("B2", "A2*2"), ("C1", "B1+B2")],
    );
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("C1"))
        .unwrap();
    overlay.edit(|edits| {
        edits.set_value(at("A1"), 1.2).unwrap();
        edits.set_value(at("A2"), 3.).unwrap();
    });
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("C1")).unwrap(), CellValue::Number(7.));
    assert_eq!(next.usage().formula_executions, 3);
}

#[test]
fn earlier_publication_preserves_the_generation_needed_by_dirty_parents() {
    let source = book(&[("A1", 1.1)], &[("B1", "ROUND(A1,0)"), ("C1", "B1+10")]);
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("C1"))
        .unwrap();
    for (input, expected, executions) in [(1.2, 11., 1), (2.2, 12., 2), (3.2, 13., 2)] {
        overlay.set_value(at("A1"), input).unwrap();
        let mut next = overlay.evaluation(&context).unwrap();
        next.cell(at("B1")).unwrap();
        assert_eq!(next.cell(at("C1")).unwrap(), CellValue::Number(expected));
        assert_eq!(next.usage().formula_executions, executions);
    }
    // A separately published change cannot disappear behind a later equal result.
    overlay.set_value(at("A1"), 4.2).unwrap();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("B1"))
        .unwrap();
    overlay.set_value(at("A1"), 4.3).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(14.)
    );
}

#[test]
fn range_membership_keeps_missing_cells_and_unchanged_formula_causes() {
    let source = book(
        &[("A1", 1.1)],
        &[("B1", "ROUND(A1,0)"), ("C1", "SUM(B1:B3)")],
    );
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("C1"))
        .unwrap();
    overlay.set_value(at("A1"), 1.2).unwrap();
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("C1")).unwrap(), CellValue::Number(1.));
    assert_eq!(next.usage().formula_executions, 1);
    drop(next);
    overlay.set_value(at("B3"), 8.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(9.)
    );
    overlay.remove_cell(at("B3")).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(1.)
    );
}

#[test]
fn clock_consumers_execute_but_unchanged_days_stop_downstream_work() {
    let source = book(&[], &[("B1", "TODAY()"), ("C1", "B1+1")]);
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let mut context = EvaluationContext {
        as_of_time: UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        ..Default::default()
    };
    let first = overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("C1"))
        .unwrap();
    context.as_of_time += Duration::from_secs(1);
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("C1")).unwrap(), first);
    assert_eq!(next.usage().formula_executions, 1);
    drop(next);
    context.as_of_time += Duration::from_secs(86_400);
    let mut next = overlay.evaluation(&context).unwrap();
    let CellValue::Number(first) = first else {
        panic!("TODAY returns a serial day")
    };
    assert_eq!(next.cell(at("C1")).unwrap(), CellValue::Number(first + 1.));
    assert_eq!(next.usage().formula_executions, 2);
}

#[test]
fn formula_metadata_changes_do_not_use_scalar_equality_as_a_certificate() {
    let source = book(&[("A1", 2.)], &[("B1", "A1"), ("C1", "SUBTOTAL(9,B1:B2)")]);
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(2.)
    );
    overlay.set_formula(at("B1"), "SUBTOTAL(9,A1:A1)").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(0.)
    );
    overlay.edit(|edits| {
        edits.set_value(at("B1"), 2.).unwrap();
        edits.set_formula(at("B1"), "SUBTOTAL(9,A1:A1)").unwrap();
        edits
            .set_style(
                at("B1"),
                CellStyle {
                    number_format: Some("0.00".into()),
                    ..Default::default()
                },
            )
            .unwrap();
    });
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(0.)
    );
}

#[test]
fn scalar_to_spill_transition_cannot_hide_behind_an_equal_anchor_value() {
    let source = book(
        &[("A1", 0.)],
        &[("C1", "IF(A1,SEQUENCE(2),1)"), ("E1", "C1+C2")],
    );
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("E1"))
            .unwrap(),
        CellValue::Number(1.)
    );
    overlay.set_value(at("A1"), 1.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("E1"))
            .unwrap(),
        CellValue::Number(3.)
    );
    overlay.set_value(at("A1"), 0.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("E1"))
            .unwrap(),
        CellValue::Number(1.)
    );
}

#[test]
fn failed_batches_do_not_publish_successful_revalidations() {
    let source = book(
        &[("A1", 1.1)],
        &[
            ("B1", "ROUND(A1,0)"),
            ("C1", "B1+1"),
            ("D1", "REPT(\"x\",100)"),
        ],
    );
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("C1"))
        .unwrap();
    overlay.set_value(at("A1"), 1.2).unwrap();
    let mut limited = context.clone();
    limited.limits = limited.limits.max_text_bytes(30);
    assert!(
        overlay
            .evaluation(&limited)
            .unwrap()
            .outputs(&[at("C1"), at("D1")])
            .is_err()
    );
    assert!(overlay.cached_value(at("B1")).is_none());
    assert!(overlay.cached_value(at("C1")).is_none());
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("C1")).unwrap(), CellValue::Number(2.));
    assert_eq!(next.usage().formula_executions, 1);
}

#[test]
fn dirty_evidence_is_not_exported_or_restored_as_current_results() {
    let source = book(
        &[("A1", 1.1)],
        &[("B1", "ROUND(A1,0)"), ("C1", "B1+1"), ("D1", "9")],
    );
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&[at("C1"), at("D1")])
        .unwrap();
    overlay.set_value(at("A1"), 2.2).unwrap();
    assert_eq!(overlay.cached_cells(), 1);
    assert_eq!(overlay.retained_usage().cells, 3);
    for change in overlay.changes() {
        if let WorkbookChange::SetCachedValue { address, .. } = change {
            assert_eq!(address, at("D1").address);
        }
    }
    let snapshot = overlay.snapshot().into_owned().unwrap();
    let (restored_book, state) = Overlay::from_snapshot(snapshot).unwrap();
    let mut restored = state.attach(&restored_book).unwrap();
    assert_eq!(restored.cached_cells(), 1);
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(3.)
    );
}

#[test]
fn source_literal_coercion_entries_cannot_validate_later_input_edits() {
    let source = book(&[("A1", f64::INFINITY)], &[("B1", "A1"), ("C1", "B1+1")]);
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Error(poirot::CellError::Num)
    );
    overlay.set_value(at("A1"), 3.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(4.)
    );
    overlay.set_value(at("A1"), f64::NEG_INFINITY).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Error(poirot::CellError::Num)
    );
}

#[test]
fn changed_lazy_branches_and_new_cycles_are_recomputed() {
    let source = book(
        &[("A1", 1.1), ("D1", 1.), ("E1", 20.)],
        &[("B1", "ROUND(A1,0)"), ("C1", "IF(D1,B1,E1)")],
    );
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("C1"))
        .unwrap();
    overlay.set_value(at("E1"), 30.).unwrap();
    assert!(overlay.cached_value(at("C1")).is_some());
    overlay.set_value(at("A1"), 1.2).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(1.)
    );
    overlay.set_value(at("D1"), 0.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(30.)
    );
    overlay.set_formula(at("E1"), "C1").unwrap();
    assert!(matches!(
        overlay.evaluation(&context).unwrap().cell(at("C1")),
        Ok(CellValue::Error(_)) | Err(_)
    ));
}

#[test]
fn dirty_storage_and_validation_obey_retention_and_depth_limits() {
    let source = book(&[("A1", 1.1)], &[("B1", "ROUND(A1,0)"), ("C1", "B1+1")]);
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("C1"))
        .unwrap();
    overlay.set_value(at("A1"), 1.2).unwrap();
    let dirty_usage = overlay.retained_usage();
    assert_eq!(dirty_usage.cells, 2);
    let mut limited = context.clone();
    limited.retention.cells = 1;
    assert!(overlay.evaluation(&limited).is_err());
    assert_eq!(overlay.retained_usage(), dirty_usage);
    limited.retention.cells = usize::MAX;
    limited.limits = limited.limits.max_depth(1);
    assert!(
        overlay
            .evaluation(&limited)
            .unwrap()
            .cell(at("C1"))
            .is_err()
    );
    assert!(overlay.cached_value(at("C1")).is_none());
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("C1")).unwrap(), CellValue::Number(2.));
    assert_eq!(next.usage().formula_executions, 1);
}

#[test]
fn many_dirty_causes_validate_unchanged_values_without_executing_the_parent() {
    let mut source = book(&[], &[("C1", "SUM(B1:B12)")]);
    for row in 1..=12 {
        let sheet = source.sheet_mut(0).unwrap();
        sheet
            .set_value(at(&format!("A{row}")).address, 1.1)
            .unwrap();
        sheet
            .set_formula(at(&format!("B{row}")).address, format!("ROUND(A{row},0)"))
            .unwrap();
    }
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("C1"))
        .unwrap();
    overlay.edit(|edits| {
        for row in 1..=12 {
            edits.set_value(at(&format!("A{row}")), 1.2).unwrap();
        }
    });
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("C1")).unwrap(), CellValue::Number(12.));
    // Every ROUND executes, but their unchanged values validate SUM's retained result.
    assert_eq!(next.usage().formula_executions, 12);
}

#[test]
fn style_edits_force_direct_readers_but_unchanged_values_stop_downstream_work() {
    let source = book(&[("A1", 1.1)], &[("B1", "ROUND(A1,0)"), ("C1", "B1+1")]);
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("C1"))
        .unwrap();
    overlay.edit(|edits| {
        edits.set_value(at("A1"), 1.2).unwrap();
        edits
            .set_style(
                at("A1"),
                CellStyle {
                    number_format: Some("0.00".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        edits.set_value(at("A1"), 1.3).unwrap();
    });
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("C1")).unwrap(), CellValue::Number(2.));
    // The style change forces direct reader B1. C1 consumes only B1's unchanged value.
    assert_eq!(next.usage().formula_executions, 1);
}

#[test]
fn previously_blank_child_readers_recalculate_when_a_scalar_producer_grows() {
    let source = book(
        &[("A1", 0.)],
        &[("C1", "IF(A1,SEQUENCE(2),1)"), ("F1", "SUM(C2:C3)")],
    );
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("F1"))
            .unwrap(),
        CellValue::Number(0.)
    );
    overlay.set_value(at("A1"), 1.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("F1"))
            .unwrap(),
        CellValue::Number(2.)
    );
    overlay.set_value(at("A1"), 0.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("F1"))
            .unwrap(),
        CellValue::Number(0.)
    );
}

#[test]
fn changed_condition_does_not_speculatively_evaluate_the_old_branch() {
    let source = book(
        &[("A1", 1.), ("A2", 1.)],
        &[
            ("B1", "A1"),
            ("D1", "REPT(\"x\",A2)"),
            ("F1", "IF(B1,D1,\"skip\")"),
        ],
    );
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("F1"))
            .unwrap(),
        CellValue::Text("x".into())
    );
    // Dirty the previous branch first, so unordered certificate validation would
    // hit its refusal before discovering that the changed condition skips it.
    overlay.set_value(at("A2"), 1_000.).unwrap();
    overlay.set_value(at("A1"), 0.).unwrap();
    let mut limited = context.clone();
    limited.limits = limited.limits.max_text_bytes(100);
    let mut next = overlay.evaluation(&limited).unwrap();
    assert_eq!(next.cell(at("F1")).unwrap(), CellValue::Text("skip".into()));
    assert_eq!(next.usage().formula_executions, 2);
}

#[test]
fn eager_error_order_precedes_a_later_dependency_refusal() {
    let source = book(
        &[("A1", 1.), ("A2", 1.)],
        &[("B1", "A1"), ("D1", "REPT(\"x\",A2)"), ("F1", "SUM(B1,D1)")],
    );
    let mut overlay = Overlay::with_workbook(source.model()).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("F1"))
            .unwrap(),
        CellValue::Number(1.)
    );
    overlay.set_value(at("A2"), 1_000.).unwrap();
    overlay
        .set_value(at("A1"), CellValue::Error(poirot::CellError::Div0))
        .unwrap();
    let mut limited = context.clone();
    limited.limits = limited.limits.max_text_bytes(100);
    let mut next = overlay.evaluation(&limited).unwrap();
    assert_eq!(
        next.cell(at("F1")).unwrap(),
        CellValue::Error(poirot::CellError::Div0)
    );
    assert_eq!(next.usage().formula_executions, 2);
}
