//! Native visual metadata participates in source preflight before XLSX serialization.
use poirot_io::{WriteOptions, plan_xlsx_export};
use poirot_model::{CellStyle, FontStyle, StyleContext, VisualStyle, Workbook};
use std::sync::Arc;

#[test]
fn export_source_budget_counts_visual_font_theme_palette_and_default_style() {
    let mut book = Workbook::new();
    let style = CellStyle {
        visual: Some(Arc::new(VisualStyle {
            font: Some(FontStyle {
                name: Some("Example".into()),
                ..Default::default()
            }),
            ..Default::default()
        })),
        ..Default::default()
    };
    book.style_context = StyleContext::new(Some(vec![1; 17]), vec![[255, 1, 2, 3]])
        .unwrap()
        .with_default_style(style.clone());
    book.add_sheet("S")
        .unwrap()
        .set_style("A1".parse().unwrap(), style)
        .unwrap();
    // Theme, palette, default font, sheet name, row presence, blank scalar, cell font.
    let expected = 17 + 4 + 7 + 1 + 24 + 8 + 7;
    assert_eq!(
        plan_xlsx_export(&book, WriteOptions::default())
            .unwrap()
            .source_bytes(),
        expected
    );
    assert!(
        plan_xlsx_export(
            &book,
            WriteOptions {
                max_source_bytes: expected - 1,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        plan_xlsx_export(
            &book,
            WriteOptions {
                max_source_bytes: expected,
                ..Default::default()
            }
        )
        .is_ok()
    );
}
