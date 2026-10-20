use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
use poirot_model::{Cell, CellAddress, CellError, CellStyle, CellValue, Workbook};
use std::io::{Cursor, Read};
fn at(s: &str) -> CellAddress {
    s.parse().unwrap()
}
#[test]
fn dynamic_metadata_typed_children_and_blank_source_style_roundtrip() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_formula_with_cached_value(at("A1"), "{1,2,3}", CellValue::Number(1.))
        .unwrap();
    sheet
        .set(
            at("B1"),
            Cell {
                style: CellStyle {
                    number_format: Some("0.00".into()),
                },
                ..Default::default()
            },
        )
        .unwrap();
    sheet
        .set_spill_cached_value(at("B1"), at("A1"), "{1,2,3}", CellValue::Number(2.))
        .unwrap();
    sheet
        .set_spill_cached_value(at("C1"), at("A1"), "{1,2,3}", CellValue::Number(3.))
        .unwrap();
    let bytes = write_xlsx_to_buffer(&book).unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("xl/worksheets/sheet1.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains(r#"<f t="array" ref="A1:C1">"#), "{xml}");
    assert!(xml.contains(r#"cm="1""#), "{xml}");
    let mut metadata = String::new();
    zip.by_name("xl/metadata.xml")
        .unwrap()
        .read_to_string(&mut metadata)
        .unwrap();
    assert!(metadata.contains("dynamicArrayProperties"));
    let reopened = read_workbook_bytes(&bytes).unwrap();
    let sheet = reopened.sheet(0).unwrap();
    assert_eq!(sheet.spill_owner(at("B1")), Some(at("A1")));
    assert_eq!(sheet.get(at("C1")).unwrap().value, CellValue::Number(3.));
    assert!(sheet.source_cell(at("C1")).is_none());
    let source = sheet.source_cell(at("B1")).unwrap();
    assert_eq!(source.value, CellValue::Blank);
    assert_eq!(source.style.number_format.as_deref(), Some("0.00"));
}
#[test]
fn modern_error_values_and_formula_caches_survive_backend_adaptation() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_value(at("A1"), CellValue::Error(CellError::Spill))
        .unwrap();
    sheet
        .set_formula_with_cached_value(at("B1"), "1+2", CellValue::Error(CellError::Calc))
        .unwrap();
    let reopened = read_workbook_bytes(&write_xlsx_to_buffer(&book).unwrap()).unwrap();
    assert_eq!(
        reopened.sheet(0).unwrap().get(at("A1")).unwrap().value,
        CellValue::Error(CellError::Spill)
    );
    assert_eq!(
        reopened.sheet(0).unwrap().get(at("B1")).unwrap().value,
        CellValue::Error(CellError::Calc)
    );
}
#[test]
fn export_refuses_authored_holes_and_overlapping_or_nested_spill_owners() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_formula_with_cached_value(at("A1"), "{1,2,3,4}", CellValue::Number(1.))
        .unwrap();
    sheet.set_value(at("C1"), 99.).unwrap();
    sheet
        .set_spill_cached_value(at("B1"), at("A1"), "{1,2,3,4}", CellValue::Number(2.))
        .unwrap();
    sheet
        .set_spill_cached_value(at("D1"), at("A1"), "{1,2,3,4}", CellValue::Number(4.))
        .unwrap();
    assert!(write_xlsx_to_buffer(&book).is_err());
    let sheet = book.sheet_mut(0).unwrap();
    sheet.remove(at("C1"));
    // Styled blanks in a sparse cache rectangle are not authored blockers.
    sheet
        .set(
            at("C1"),
            Cell {
                style: CellStyle {
                    number_format: Some("0.00".into()),
                },
                ..Default::default()
            },
        )
        .unwrap();
    assert!(write_xlsx_to_buffer(&book).is_ok());
    let sheet = book.sheet_mut(0).unwrap();
    sheet
        .set_formula_with_cached_value(at("C1"), "{9;10}", CellValue::Number(9.))
        .unwrap();
    sheet
        .set_spill_cached_value(at("C2"), at("C1"), "{9;10}", CellValue::Number(10.))
        .unwrap();
    assert!(write_xlsx_to_buffer(&book).is_err());
}
