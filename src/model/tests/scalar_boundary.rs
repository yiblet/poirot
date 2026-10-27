use super::*;

#[test]
fn formula_cache_and_source_remain_separate() {
    let mut sheet = crate::model::Worksheet::new("Data").unwrap();
    let address = crate::model::CellAddress::new(0, 0).unwrap();
    let value = CellValue::from(ExcelText::from_units(vec![0xd83d]));
    sheet
        .set_formula_with_cached_value(address, "LEFT(\"😀\",1)", value.clone())
        .unwrap();
    assert_eq!(sheet.get(address).unwrap().value, value);
    assert_eq!(
        sheet.get(address).unwrap().formula.as_deref(),
        Some("LEFT(\"😀\",1)")
    );
    sheet.set_value(address, "😀").unwrap();
    assert!(matches!(
        sheet.get(address).unwrap().value,
        CellValue::Text(_)
    ));
}
