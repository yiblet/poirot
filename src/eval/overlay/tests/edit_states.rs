//! Authored edit transitions preserve baseline inheritance, explicit blanks and snapshots.
//! Expected source states are defined by the public edit operations, not engine output.
use crate::eval::config::EvaluationContext;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::overlay::evaluation::Evaluation;
use crate::eval::overlay::incremental::Overlay;
use crate::model::{CellStyle, CellValue, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

fn style(format: &str) -> CellStyle {
    CellStyle {
        number_format: Some(format.into()),
        ..Default::default()
    }
}

fn book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value(at("A1").address, 7.).unwrap();
    sheet.set_formula(at("B1").address, "A1+1").unwrap();
    book
}

#[test]
fn style_delete_style_is_a_blank_replacement_and_survives_detached_projection() {
    let mut overlay = Overlay::with_workbook(book()).build().unwrap();
    overlay.set_style(at("A1"), style("0.0")).unwrap();
    assert_eq!(overlay.edited_cells(), 1);
    overlay.remove_cell(at("A1")).unwrap();
    assert!(overlay.source_cell(at("A1")).is_none());
    overlay.set_style(at("A1"), style("0.00")).unwrap();
    let source = overlay.source_cell(at("A1")).unwrap();
    assert_eq!(source.value, CellValue::Blank);
    assert_eq!(source.style, style("0.00"));
    assert_eq!(overlay.edited_cells(), 1);
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (baseline, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let projected = state.source_workbook(&baseline).unwrap();
    assert_eq!(
        projected
            .sheet(0)
            .unwrap()
            .source_cell(at("A1").address)
            .unwrap()
            .value,
        CellValue::Blank
    );
    let mut restored = state.attach(baseline).unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("B1"))
            .unwrap(),
        1.0.into()
    );
    assert_eq!(restored.flush().unwrap(), 1);
    restored.reset();
    assert_eq!(
        restored.source_cell(at("A1")).unwrap().value,
        CellValue::Blank
    );
    assert_eq!(restored.source_cell(at("A1")).unwrap().style, style("0.00"));
}

#[test]
fn deletion_blank_and_restoring_baseline_are_distinct_in_both_workspaces() {
    let book = book();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    overlay.remove_cell(at("A1")).unwrap();
    overlay.remove_cell(at("A1")).unwrap();
    assert_eq!(overlay.edited_cells(), 1);
    overlay.set_value(at("A1"), CellValue::Blank).unwrap();
    assert_eq!(
        overlay.source_cell(at("A1")).unwrap().value,
        CellValue::Blank
    );
    assert_eq!(overlay.edited_cells(), 1);
    overlay.set_value(at("A1"), 9.).unwrap();
    overlay.set_style(at("A1"), style("0.00")).unwrap();
    overlay.set_value(at("A1"), 7.).unwrap();
    assert_eq!(overlay.edited_cells(), 1); // The style still differs.
    overlay.set_style(at("A1"), CellStyle::default()).unwrap();
    assert_eq!(overlay.edited_cells(), 0);
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    direct.remove_cell(at("A1")).unwrap();
    direct.set_value(at("A1"), CellValue::Blank).unwrap();
    assert_eq!(
        direct.source_cell(at("A1")).unwrap().value,
        CellValue::Blank
    );
    assert_eq!(direct.cell(at("B1")).unwrap(), 1.0.into());
    direct.set_value(at("A1"), 7.).unwrap();
    assert_eq!(direct.edited_cells(), 0);
    assert_eq!(direct.cell(at("B1")).unwrap(), 8.0.into());
}

#[test]
fn mixed_tombstones_style_and_explicit_blank_export_and_reset_coherently() {
    let mut baseline = book();
    baseline
        .sheet_mut(0)
        .unwrap()
        .set_value(at("A3").address, 3.)
        .unwrap();
    let mut overlay = Overlay::with_workbook(&baseline).build().unwrap();
    overlay.remove_cell(at("A1")).unwrap();
    overlay.set_style(at("B1"), style("0.00")).unwrap();
    overlay.set_value(at("A3"), CellValue::Blank).unwrap();
    overlay.set_formula(at("C1"), "B1+A3").unwrap();
    assert_eq!(overlay.edited_cells(), 4);
    let mut exported = baseline.clone();
    exported.apply(overlay.changes()).unwrap();
    assert!(
        exported
            .sheet(0)
            .unwrap()
            .source_cell(at("A1").address)
            .is_none()
    );
    assert_eq!(
        exported
            .sheet(0)
            .unwrap()
            .source_cell(at("A3").address)
            .unwrap()
            .value,
        CellValue::Blank
    );
    assert_eq!(
        exported
            .sheet(0)
            .unwrap()
            .source_cell(at("B1").address)
            .unwrap()
            .style,
        style("0.00")
    );
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (book, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.into_evaluation(book, Default::default()).unwrap();
    assert_eq!(restored.edited_cells(), 4);
    assert_eq!(restored.cell(at("C1")).unwrap(), 1.0.into());
    restored.reset();
    assert_eq!(restored.edited_cells(), 0);
    assert_eq!(restored.cell(at("B1")).unwrap(), 8.0.into());
    assert!(restored.source_cell(at("C1")).is_none());
}
