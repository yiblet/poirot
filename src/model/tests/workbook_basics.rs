use super::*;

#[test]
fn addresses_round_trip_and_validate() {
    for (text, row, col) in [
        ("A1", 0, 0),
        ("Z9", 8, 25),
        ("AA1", 0, 26),
        ("XFD1048576", MAX_ROWS - 1, MAX_COLUMNS - 1),
    ] {
        let a: CellAddress = text.parse().unwrap();
        assert_eq!(a, CellAddress::new(row, col).unwrap());
        assert_eq!(a.to_string(), text);
    }
    assert_eq!("$a$1".parse::<CellAddress>().unwrap().to_string(), "A1");
    for text in [
        "A0",
        "XFE1",
        "A1048577",
        "1A",
        "A",
        "A-1",
        "A1junk",
        "💥1",
        "A42949672960",
    ] {
        assert!(text.parse::<CellAddress>().is_err(), "{text}");
    }
}

#[test]
fn sheet_names_and_lookup() {
    let mut w = Workbook::new();
    w.add_sheet("Data").unwrap();
    assert!(w.add_sheet("DATA").is_err());
    assert!(w.sheet_by_name("data").is_some());
    for name in ["", "bad/name", "'quoted", "a:b"] {
        assert!(w.add_sheet(name).is_err());
    }
    assert!(w.add_sheet("😀".repeat(16)).is_err());
    w.rename_sheet(0, "Results").unwrap();
    assert_eq!(w.sheet(0).unwrap().name(), "Results");
    assert!(w.rename_sheet(4, "Other").is_err());
}

#[test]
fn formulas_preserve_cache_and_style() {
    let mut s = Worksheet::new("Sheet1").unwrap();
    let a = "A1".parse().unwrap();
    s.set_formula_with_cached_value(a, " = SUM(B1:B2) ", CellValue::Number(3.0))
        .unwrap();
    let c = s.get(a).unwrap();
    assert_eq!(c.formula.as_deref(), Some("SUM(B1:B2)"));
    assert_eq!(c.value, CellValue::Number(3.0));
    assert!(s.set_formula(a, "=").is_err());
    assert_eq!(s.get(a).unwrap().value, CellValue::Number(3.0));
    s.get_mut(a).unwrap().style.number_format = Some("0.00".into());
    s.set_value(a, true).unwrap();
    assert_eq!(s.get(a).unwrap().formula, None);
    assert_eq!(
        s.get(a).unwrap().style.number_format.as_deref(),
        Some("0.00")
    );
    assert!(
        s.set_value(
            CellAddress {
                row: MAX_ROWS,
                col: 0
            },
            1.0
        )
        .is_err()
    );
}

#[test]
fn errors_round_trip() {
    for error in [
        CellError::Null,
        CellError::Div0,
        CellError::Value,
        CellError::Ref,
        CellError::Name,
        CellError::Num,
        CellError::NA,
        CellError::GettingData,
        CellError::Spill,
        CellError::Calc,
    ] {
        assert_eq!(error.to_string().parse::<CellError>().unwrap(), error);
    }
    assert!("nope".parse::<CellError>().is_err());
}

#[test]
fn names_resolve_local_before_global() {
    let mut w = Workbook::new();
    w.add_sheet("Data").unwrap();
    w.define_name("Rate", "=0.1", None).unwrap();
    w.define_name("Rate", "0.2", Some(0)).unwrap();
    assert_eq!(w.defined_name("RATE", 0).unwrap().formula, "0.2");
    assert!(w.define_name("A1", "1", None).is_err());
    assert!(w.define_name("Rate", "1", None).is_err());
    assert!(w.define_name("Other", "1", Some(1)).is_err());
}
