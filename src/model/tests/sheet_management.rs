use super::*;

#[test]
fn reorder_remaps_scopes_and_duplicate_retains_typed_sources_and_presentation() {
    let mut w = Workbook::new();
    for name in ["A", "B", "C"] {
        w.add_sheet(name).unwrap();
    }
    w.define_name("local", "A1", Some(0)).unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), ExcelText::from_units(vec![0xd800]))
        .unwrap();
    w.sheet_mut(0).unwrap().set_presentation(SheetPresentation {
        visibility: SheetVisibility::VeryHidden,
        tab_color: Some([1, 2, 3]),
    });
    let copy = w.duplicate_sheet(0, "Copy").unwrap();
    assert_eq!(copy, 3);
    assert_eq!(
        w.sheet(copy).unwrap().presentation(),
        SheetPresentation {
            visibility: SheetVisibility::Visible,
            tab_color: Some([1, 2, 3])
        }
    );
    assert_eq!(
        w.sheet(copy).unwrap().get("A1".parse().unwrap()),
        w.sheet(0).unwrap().get("A1".parse().unwrap())
    );
    assert!(w.named_ranges.iter().any(|n| n.sheet_scope == Some(3)));
    w.move_sheet(0, 3).unwrap();
    assert_eq!(
        w.sheets().iter().map(|s| s.name()).collect::<Vec<_>>(),
        ["B", "C", "Copy", "A"]
    );
    assert_eq!(
        w.named_ranges
            .iter()
            .map(|n| n.sheet_scope)
            .collect::<Vec<_>>(),
        [Some(3), Some(2)]
    );
    w.move_sheet(3, 0).unwrap();
    let original = w.clone();
    assert!(w.move_sheet(0, 99).is_err());
    assert!(w.duplicate_sheet(0, "a").is_err());
    assert!(w.duplicate_sheet(0, "bad/name").is_err());
    assert_eq!(w, original);
}
