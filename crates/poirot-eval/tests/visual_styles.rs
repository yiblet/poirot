//! Native visual presentation survives sparse edits without changing calculation semantics.
use poirot_eval::{
    EvaluationTarget,
    overlay::{EvaluationContext, Overlay},
};
use poirot_model::{CellStyle, CellValue, FontSize, FontStyle, VisualStyle, Workbook};
use std::sync::Arc;

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
fn style() -> CellStyle {
    CellStyle {
        visual: Some(Arc::new(VisualStyle {
            font: Some(FontStyle {
                name: Some("Example".into()),
                size: Some(FontSize::new(11.5).unwrap()),
                bold: true,
                ..Default::default()
            }),
            ..Default::default()
        })),
        ..Default::default()
    }
}
#[test]
fn visual_only_edits_keep_values_and_survive_reset_flush_and_snapshot() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value(at("A1").address, 7.).unwrap();
    sheet
        .set_formula_with_cached_value(at("B1").address, "A1+1", 999.0.into())
        .unwrap();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B1"))
            .unwrap(),
        8.0.into()
    );
    overlay.set_style(at("B1"), style()).unwrap();
    overlay.set_style(at("C1"), style()).unwrap();
    let output = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&[at("B1"), at("C1")])
        .unwrap();
    assert_eq!(output[0].value, 8.0.into());
    assert_eq!(output[1].value, CellValue::Blank);
    assert_eq!(output[0].formatting, Some(style()));
    assert_eq!(output[1].formatting, Some(style()));
    assert_eq!(overlay.source_cell(at("B1")).unwrap().value, 999.0.into());
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (baseline, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach(baseline).unwrap();
    assert_eq!(restored.source_cell(at("C1")).unwrap().style, style());
    assert_eq!(restored.flush().unwrap(), 2);
    restored.set_style(at("B1"), CellStyle::default()).unwrap();
    restored.reset();
    assert_eq!(restored.source_cell(at("B1")).unwrap().style, style());
    assert_eq!(restored.source_cell(at("C1")).unwrap().style, style());
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("B1"))
            .unwrap(),
        8.0.into()
    );
}
