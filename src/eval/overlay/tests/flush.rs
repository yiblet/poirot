use super::super::incremental::Overlay;
use super::incremental::{at, book, dynamic, value};
use crate::eval::config::EvaluationContext;
use crate::eval::coordinates::EvaluationTarget;
use crate::model::{CellAddress, CellStyle, CellValue};
use std::sync::Arc;

#[test]
fn owned_flush_preserves_allocations_bytecode_results_and_followup_dependencies() {
    let mut overlay = Overlay::with_workbook(book()).build().unwrap();
    let context = EvaluationContext::default();
    overlay.evaluation(&context).unwrap().all().unwrap();
    let formula_ptr = overlay
        .workbook()
        .sheet(0)
        .unwrap()
        .get(at("D1").address)
        .unwrap()
        .formula
        .as_ref()
        .unwrap()
        .as_ptr();
    let result = overlay.cached_value(at("D1")).unwrap();
    overlay.set_value(at("A1"), 8.).unwrap();
    overlay.set_formula(at("B1"), "=A1*3").unwrap();
    overlay
        .set_style(
            at("A1"),
            CellStyle {
                number_format: Some("0.00".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(value(&mut overlay, "C1", &context), CellValue::Number(25.));
    let compiled = overlay.compiled_usage();
    let cached = overlay.cached_cells();
    assert_eq!(overlay.flush().unwrap(), 2);
    assert_eq!(overlay.edited_cells(), 0);
    assert_eq!(overlay.flush().unwrap(), 0);
    assert_eq!(overlay.cached_cells(), cached);
    assert_eq!(overlay.compiled_usage(), compiled);
    assert!(Arc::ptr_eq(
        &result,
        &overlay.cached_value(at("D1")).unwrap()
    ));
    let sheet = overlay.workbook().sheet(0).unwrap();
    assert_eq!(
        sheet
            .get(at("D1").address)
            .unwrap()
            .formula
            .as_ref()
            .unwrap()
            .as_ptr(),
        formula_ptr
    );
    assert_eq!(
        sheet
            .get(at("A1").address)
            .unwrap()
            .style
            .number_format
            .as_deref(),
        Some("0.00")
    );
    assert_eq!(
        sheet.get(at("B1").address).unwrap().formula.as_deref(),
        Some("A1*3")
    );
    // Source publication deliberately does not materialize evaluated caches.
    assert_eq!(sheet.get(at("B1").address).unwrap().value, CellValue::Blank);
    let mut warm = overlay.evaluation(&context).unwrap();
    warm.all().unwrap();
    assert_eq!(warm.usage().formula_executions, 0);
    drop(warm);
    overlay.set_value(at("A1"), 4.).unwrap();
    assert!(overlay.cached_value(at("C1")).is_none());
    assert!(overlay.cached_value(at("D1")).is_some());
    assert_eq!(value(&mut overlay, "C1", &context), CellValue::Number(13.));
    assert_eq!(overlay.flush().unwrap(), 1);
    overlay.remove_cell(at("A1")).unwrap();
    assert_eq!(overlay.flush().unwrap(), 1);
    assert!(
        overlay
            .workbook()
            .sheet(0)
            .unwrap()
            .get(at("A1").address)
            .is_none()
    );
    assert_eq!(value(&mut overlay, "C1", &context), CellValue::Number(1.));
    assert_eq!(overlay.flush().unwrap(), 0);
}

#[test]
fn exclusive_borrow_flush_and_rejected_edits_are_atomic() {
    let mut workbook = book();
    {
        let mut overlay = Overlay::with_workbook(&mut workbook).build().unwrap();
        overlay.set_value(at("A1"), 10.).unwrap();
        let before = postcard::to_allocvec(&overlay.snapshot()).unwrap();
        assert!(overlay.set_formula(at("B1"), "= ").is_err());
        assert!(
            overlay
                .set_value(
                    EvaluationTarget {
                        sheet: 9,
                        ..at("A1")
                    },
                    5.
                )
                .is_err()
        );
        assert!(
            overlay
                .set_value(
                    EvaluationTarget {
                        sheet: 0,
                        address: CellAddress {
                            row: u32::MAX,
                            col: 0
                        }
                    },
                    5.
                )
                .is_err()
        );
        assert_eq!(postcard::to_allocvec(&overlay.snapshot()).unwrap(), before);
        assert_eq!(overlay.flush().unwrap(), 1);
        assert_eq!(overlay.flush().unwrap(), 0);
    }
    assert_eq!(
        workbook
            .sheet(0)
            .unwrap()
            .get(at("A1").address)
            .unwrap()
            .value,
        CellValue::Number(10.)
    );
}

#[test]
fn flush_failure_leaves_source_edits_and_retained_state_untouched() {
    // Public setters validate immediately; exercise the publication guard with
    // corrupted private staging to ensure it remains transactional defensively.
    let mut overlay = Overlay::with_workbook(book()).build().unwrap();
    let context = EvaluationContext::default();
    value(&mut overlay, "C1", &context);
    overlay.set_value(at("A1"), 5.).unwrap();
    overlay
        .source
        .inputs
        .sheets
        .entry(99)
        .or_default()
        .set(at("A1").address, crate::eval::inputs::CellEdit::Delete)
        .unwrap();
    let before = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    assert!(overlay.flush().is_err());
    assert_eq!(postcard::to_allocvec(&overlay.snapshot()).unwrap(), before);
}

#[test]
fn flush_spills_shrink_blockers_snapshots_and_saved_cache_export() {
    let mut workbook = book();
    let sheet = workbook.sheet_mut(0).unwrap();
    sheet
        .set_formula_with_cached_value(at("G1").address, "IF(A1,{1,2;3,4},{9})", 1.0.into())
        .unwrap();
    sheet
        .set_spill_cached_value(
            at("H2").address,
            at("G1").address,
            "IF(A1,{1,2;3,4},{9})",
            4.0.into(),
        )
        .unwrap();
    let mut overlay = Overlay::with_workbook(workbook)
        .with_policy(dynamic())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    overlay.set_formula(at("J1"), "SUM(H1:H2)").unwrap();
    assert_eq!(value(&mut overlay, "J1", &context), CellValue::Number(6.));
    overlay.flush().unwrap();
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (workbook, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut overlay = state.attach(workbook).unwrap();
    let mut warm = overlay.evaluation(&context).unwrap();
    assert_eq!(warm.cell(at("H2")).unwrap(), CellValue::Number(4.));
    assert_eq!(warm.usage().formula_executions, 0);
    drop(warm);
    overlay.set_value(at("H2"), 99.).unwrap();
    overlay.flush().unwrap();
    assert_eq!(
        overlay
            .workbook()
            .sheet(0)
            .unwrap()
            .spill_owner(at("H2").address),
        None
    );
    assert_eq!(
        value(&mut overlay, "G1", &context),
        CellValue::Error(crate::model::CellError::Spill)
    );
    overlay.remove_cell(at("H2")).unwrap();
    overlay.flush().unwrap();
    assert_eq!(value(&mut overlay, "J1", &context), CellValue::Number(6.));
    overlay.set_value(at("A1"), false).unwrap();
    overlay.flush().unwrap();
    assert_eq!(value(&mut overlay, "H2", &context), CellValue::Blank);
    assert_eq!(value(&mut overlay, "J1", &context), CellValue::Number(0.));
    let changes = overlay.changes();
    let mut saved = overlay.workbook().clone();
    saved.apply(changes).unwrap();
    assert!(saved.sheet(0).unwrap().get(at("H2").address).is_none());
    assert_eq!(
        saved.sheet(0).unwrap().get(at("G1").address).unwrap().value,
        CellValue::Number(9.)
    );
    overlay.set_formula(at("G1"), "{5,6;7,8}").unwrap();
    assert_eq!(value(&mut overlay, "H2", &context), CellValue::Number(8.));
    overlay.flush().unwrap();
    assert!(
        overlay
            .workbook()
            .sheet(0)
            .unwrap()
            .get(at("H2").address)
            .is_none()
    );
    let mut saved = overlay.workbook().clone();
    saved.apply(overlay.changes()).unwrap();
    assert_eq!(
        saved.sheet(0).unwrap().spill_owner(at("H2").address),
        Some(at("G1").address)
    );
    assert!(
        saved
            .sheet(0)
            .unwrap()
            .source_cell(at("H2").address)
            .is_none()
    );
    overlay.remove_cell(at("G1")).unwrap();
    overlay.flush().unwrap();
    assert_eq!(value(&mut overlay, "H2", &context), CellValue::Blank);
}
