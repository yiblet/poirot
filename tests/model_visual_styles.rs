//! Authored presentation contracts, independent of spreadsheet file adapters.
use poirot::model::*;
use std::sync::Arc;

fn rich() -> CellStyle {
    CellStyle {
        number_format: Some("0.00".into()),
        visual: Some(Arc::new(VisualStyle {
            font: Some(FontStyle {
                name: Some("Example Sans".into()),
                size: Some(FontSize::new(11.5).unwrap()),
                bold: true,
                italic: true,
                underline: Some(Underline::Double),
                strike: true,
                color: Some(StyleColor {
                    source: ColorSource::Theme(ThemeColor::Accent2),
                    tint: Some(Tint::new(-0.25).unwrap()),
                }),
                scheme: Some(FontScheme::Minor),
                family: Some(FontFamily::Swiss),
                charset: Some(1),
            }),
            fill: Some(PatternFill {
                pattern: PatternType::Solid,
                foreground: Some(StyleColor {
                    source: ColorSource::Argb([255, 12, 34, 56]),
                    tint: None,
                }),
                background: Some(StyleColor {
                    source: ColorSource::Indexed(IndexedColor::new(5).unwrap()),
                    tint: None,
                }),
            }),
            borders: Some(Borders {
                left: Some(BorderSide {
                    line: Some(BorderLine::Double),
                    color: Some(StyleColor {
                        source: ColorSource::Automatic,
                        tint: None,
                    }),
                }),
                ..Default::default()
            }),
            alignment: Some(Alignment {
                horizontal: Some(HorizontalAlignment::Center),
                vertical: Some(VerticalAlignment::Top),
                wrap_text: true,
                shrink_to_fit: true,
                rotation: Some(TextRotation::Degrees(RotationDegrees::new(-45).unwrap())),
                indent: 2,
                reading_order: Some(ReadingOrder::RightToLeft),
            }),
        })),
    }
}

#[test]
fn shared_styles_keep_value_equality_and_compact_cell_storage() {
    let first = rich();
    let second = rich();
    assert_eq!(first, second);
    assert!(!Arc::ptr_eq(
        first.visual.as_ref().unwrap(),
        second.visual.as_ref().unwrap()
    ));
    let clone = first.clone();
    assert!(Arc::ptr_eq(
        first.visual.as_ref().unwrap(),
        clone.visual.as_ref().unwrap()
    ));
    let old = std::mem::size_of::<Option<String>>();
    let new = std::mem::size_of::<CellStyle>();
    eprintln!("CellStyle bytes: former {old}, shared visual {new}");
    assert_eq!(new, old + std::mem::size_of::<Option<Arc<VisualStyle>>>());
}

#[test]
fn source_snapshot_preserves_styles_blanks_context_and_sharing() {
    let mut book = Workbook::new();
    book.style_context =
        StyleContext::new(Some(b"theme metadata".to_vec()), vec![[255, 1, 2, 3]]).unwrap();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_style("A1".parse().unwrap(), rich()).unwrap();
    sheet.set_style("B1".parse().unwrap(), rich()).unwrap();
    let wire = postcard::to_allocvec(&snapshot::Snapshot::capture(&book)).unwrap();
    let restored = postcard::from_bytes::<snapshot::Snapshot<'_>>(&wire)
        .unwrap()
        .restore()
        .unwrap();
    assert_eq!(restored, book);
    let sheet = restored.sheet(0).unwrap();
    let a = sheet.get("A1".parse().unwrap()).unwrap();
    let b = sheet.get("B1".parse().unwrap()).unwrap();
    assert_eq!(a.value, CellValue::Blank);
    assert!(Arc::ptr_eq(
        a.style.visual.as_ref().unwrap(),
        b.style.visual.as_ref().unwrap()
    ));
    assert_eq!(
        clone_footprint::CloneFootprint::measure(&book)
            .unwrap()
            .text_bytes,
        1 + 2 * (4 + 12) + 14 + 4
    );
}

#[test]
fn invalid_values_and_forged_wire_values_are_rejected() {
    for bad in [f64::NAN, f64::INFINITY, -1., 0.] {
        assert!(FontSize::new(bad).is_err());
        assert!(postcard::from_bytes::<FontSize>(&postcard::to_allocvec(&bad).unwrap()).is_err());
    }
    for bad in [f64::NAN, f64::NEG_INFINITY, -1.01, 1.01] {
        assert!(Tint::new(bad).is_err());
        assert!(postcard::from_bytes::<Tint>(&postcard::to_allocvec(&bad).unwrap()).is_err());
    }
    assert_eq!(Tint::new(-0.).unwrap(), Tint::new(0.).unwrap());
    for bad in [-91i16, 91] {
        assert!(
            postcard::from_bytes::<RotationDegrees>(&postcard::to_allocvec(&bad).unwrap()).is_err()
        );
    }
    assert!(postcard::from_bytes::<IndexedColor>(&postcard::to_allocvec(&66u8).unwrap()).is_err());
    assert!(StyleContext::new(None, vec![[0; 4]; 65]).is_err());
    let forged =
        postcard::to_allocvec(&(None::<Vec<u8>>, vec![[0u8; 4]; 65], CellStyle::default()))
            .unwrap();
    assert!(postcard::from_bytes::<StyleContext>(&forged).is_err());
}

#[test]
fn saved_spill_source_keeps_visual_style_after_native_restore_and_clear() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    let anchor = "A1".parse().unwrap();
    let child = "A2".parse().unwrap();
    sheet.set_formula(anchor, "{1;2}").unwrap();
    sheet.set_style(child, rich()).unwrap();
    sheet
        .set_spill_cached_value(child, anchor, "{1;2}", 2.0.into())
        .unwrap();
    assert_eq!(
        clone_footprint::CloneFootprint::measure(&book)
            .unwrap()
            .text_bytes,
        1 + 5 + 2 * (4 + 12)
    );
    for address in [anchor, child] {
        let assignments = [(0, address, 9.0.into())];
        let projected =
            clone_footprint::CloneFootprint::measure_with_assignments(&book, &assignments).unwrap();
        let mut staged = book.clone();
        staged.sheet_mut(0).unwrap().set_value(address, 9.).unwrap();
        assert_eq!(
            projected,
            clone_footprint::CloneFootprint::measure(&staged).unwrap()
        );
    }
    let bytes = postcard::to_allocvec(&snapshot::Snapshot::capture(&book)).unwrap();
    let mut restored = postcard::from_bytes::<snapshot::Snapshot<'_>>(&bytes)
        .unwrap()
        .restore()
        .unwrap();
    let sheet = restored.sheet_mut(0).unwrap();
    assert_eq!(sheet.source_cell(child).unwrap().style, rich());
    assert_eq!(sheet.source_cell(child).unwrap().value, CellValue::Blank);
    sheet.set_value(anchor, 9.).unwrap();
    assert_eq!(sheet.source_cell(child).unwrap().style, rich());
    assert_eq!(sheet.source_cell(child).unwrap().value, CellValue::Blank);
}
