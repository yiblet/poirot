//! Shared-formula provenance lifecycle, independent of formula evaluation.
use poirot_model::{
    Cell, CellAddress, CellStyle, CellValue, MAX_ROWS, SharedFormula, SharedFormulaId, Workbook,
    WorkbookChange,
};

fn at(text: &str) -> CellAddress {
    text.parse().unwrap()
}
fn shared() -> SharedFormula {
    SharedFormula {
        group: SharedFormulaId(7),
        origin: at("B1"),
    }
}
fn workbook() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    // Original synthetic shared group: copying A1+1 one row down gives A2+1.
    for (address, formula) in [("B1", "A1+1"), ("B2", "A2+1")] {
        let mut cell = Cell::with_formula(formula, CellValue::Number(2.)).unwrap();
        cell.shared_formula = Some(shared());
        sheet.set(at(address), cell).unwrap();
    }
    book
}
#[test]
fn shared_provenance_defaults_absent() {
    assert_eq!(Cell::default().shared_formula, None);
    assert_eq!(Cell::new(1.).shared_formula, None);
    assert_eq!(
        Cell::with_formula("1", CellValue::Blank)
            .unwrap()
            .shared_formula,
        None
    );
    assert!(shared().is_anchor(at("B1")));
    assert!(!shared().is_anchor(at("B2")));
}
#[test]
fn replacing_anchor_keeps_follower_source() {
    for remove in [false, true] {
        let mut book = workbook();
        let sheet = book.sheet_mut(0).unwrap();
        let follower = sheet.get(at("B2")).unwrap().clone();
        if remove {
            sheet.remove(at("B1"));
        } else {
            sheet.set_formula(at("B1"), "99").unwrap();
        }
        assert_eq!(sheet.get(at("B2")), Some(&follower));
        assert_eq!(book.clone(), book);
        assert_eq!(Workbook::from_snapshot(book.snapshot()).unwrap(), book);
    }
}
#[test]
fn formula_and_literal_edits_detach_only_target() {
    for literal in [false, true] {
        let mut book = workbook();
        let sheet = book.sheet_mut(0).unwrap();
        let anchor = sheet.get(at("B1")).unwrap().clone();
        if literal {
            sheet.set_value(at("B2"), 3.).unwrap();
        } else {
            sheet
                .set_formula_with_cached_value(at("B2"), "A2+1", 3.0.into())
                .unwrap();
        }
        assert_eq!(sheet.get(at("B2")).unwrap().shared_formula, None);
        assert_eq!(sheet.get(at("B1")), Some(&anchor));
    }
}
#[test]
fn cache_and_style_edits_preserve_shared_provenance() {
    let mut book = workbook();
    book.apply([
        WorkbookChange::SetCachedValue {
            sheet: 0,
            address: at("B1"),
            expected_formula: "A1+1".into(),
            value: 8.0.into(),
        },
        WorkbookChange::SetStyle {
            sheet: 0,
            address: at("B2"),
            style: CellStyle {
                number_format: Some("0.00".into()),
            },
        },
    ])
    .unwrap();
    for addr in ["B1", "B2"] {
        assert_eq!(
            book.sheet(0).unwrap().get(at(addr)).unwrap().shared_formula,
            Some(shared())
        );
    }
    let before = book.clone();
    assert!(
        book.apply([
            WorkbookChange::SetFormula {
                sheet: 0,
                address: at("B1"),
                formula: "4".into()
            },
            WorkbookChange::SetFormula {
                sheet: 0,
                address: at("B2"),
                formula: "=".into()
            },
        ])
        .is_err()
    );
    assert_eq!(book, before);
}
#[test]
fn shared_provenance_snapshot_roundtrip() {
    let book = workbook();
    assert_eq!(book.snapshot().into_owned().restore().unwrap(), book);
    let bytes = postcard::to_allocvec(&book.snapshot()).unwrap();
    let restored = Workbook::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    assert_eq!(restored, book);
    assert!(restored.snapshot().matches(&book));
    let mut changed = restored.clone();
    changed
        .sheet_mut(0)
        .unwrap()
        .get_mut(at("B2"))
        .unwrap()
        .shared_formula = None;
    assert!(!book.snapshot().matches(&changed));
}
#[test]
fn malformed_shared_provenance_snapshot_rejected() {
    for missing_formula in [false, true] {
        let mut book = workbook();
        let cell = book.sheet_mut(0).unwrap().get_mut(at("B2")).unwrap();
        if missing_formula {
            cell.formula = None;
        } else {
            cell.shared_formula.as_mut().unwrap().origin.row = MAX_ROWS;
        }
        let bytes = postcard::to_allocvec(&book.snapshot()).unwrap();
        assert!(Workbook::from_snapshot(postcard::from_bytes(&bytes).unwrap()).is_err());
    }
}
#[test]
fn invalid_shared_provenance_replacement_is_atomic() {
    let mut book = workbook();
    let before = book.clone();
    let mut invalid = Cell::new(2.);
    invalid.shared_formula = Some(shared());
    assert!(
        book.replace_cells([
            (0, at("B1"), Some(Cell::new(9.))),
            (0, at("B2"), Some(invalid.clone()))
        ])
        .is_err()
    );
    assert_eq!(book, before);
    assert!(book.sheet_mut(0).unwrap().set(at("B1"), invalid).is_err());
    assert_eq!(book, before);
}

#[test]
fn rename_formula_rewrites_detach_changed_sources_and_check_provenance() {
    use poirot_model::{FormulaReplacement, FormulaSource, RenameSheetPlan};
    let mut book = workbook();
    let edits = vec![FormulaReplacement {
        source: FormulaSource::Cell {
            sheet: 0,
            address: at("B1"),
        },
        expected: "A1+1".into(),
        replacement: "A1+2".into(),
    }];
    let plan = RenameSheetPlan::new(&book, 0, "Renamed".into(), edits).unwrap();
    let mut stale = book.clone();
    stale
        .sheet_mut(0)
        .unwrap()
        .get_mut(at("B2"))
        .unwrap()
        .shared_formula = None;
    let before = stale.clone();
    assert!(stale.apply_rename_sheet_plan(&plan).is_err());
    assert_eq!(stale, before);
    book.apply_rename_sheet_plan(&plan).unwrap();
    assert_eq!(
        book.sheet(0).unwrap().get(at("B1")).unwrap().shared_formula,
        None
    );
    assert_eq!(
        book.sheet(0).unwrap().get(at("B2")).unwrap().shared_formula,
        Some(shared())
    );
}
