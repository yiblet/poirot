//! Locally authored SpreadsheetML fixtures: ECMA-376 CT_Cell, CT_Row, CT_Xf,
//! shared formula linkage, OPC parts, and XML namespace semantics.
use poirot::model::{CellAddress, CellValue, Workbook};
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};
fn at(s: &str) -> CellAddress {
    s.parse().unwrap()
}
fn parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut entry = zip.by_index(i).unwrap();
            let name = entry.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}
fn pack(parts: BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
fn fixture(sheet: &str) -> Vec<u8> {
    let mut model = Workbook::new();
    model
        .add_sheet("Data")
        .unwrap()
        .set_value(at("A1"), 1.)
        .unwrap();
    let mut package = parts(&write_xlsx_to_buffer(&model).unwrap());
    package.insert("xl/worksheets/sheet1.xml".into(), sheet.as_bytes().to_vec());
    // Opaque feature payloads exercise fresh-save omission independently of parser support.
    for (name, data) in [
        (
            "xl/charts/chart1.xml",
            "<chartSpace xmlns=\"urn:chart\"><opaque/></chartSpace>",
        ),
        (
            "xl/drawings/drawing1.xml",
            "<drawing xmlns=\"urn:drawing\"/>",
        ),
        ("xl/vbaProject.bin", "macro payload"),
        (
            "xl/worksheets/_rels/sheet1.xml.rels",
            "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"drawing\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing\" Target=\"../drawings/drawing1.xml\"/></Relationships>",
        ),
    ] {
        package.insert(name.into(), data.as_bytes().to_vec());
    }
    pack(package)
}
const SHEET: &str = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:x="urn:unknown"><dimension ref="A1:C2"/><sheetData><row r="1" ht="31" customHeight="1" x:flag="row"><c r="A1" x:flag="cell"><v>1</v><extLst><x:extra/></extLst></c><c r="B1" t="inlineStr"><is><r><rPr><b/></rPr><t>Rich</t></r><r><t> text</t></r></is></c><c r="C1"><f>1+2</f><v>3</v></c></row><row r="2"><c r="A2"><v>2</v></c></row></sheetData><drawing xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="drawing"/><extLst><x:preserve/></extLst></worksheet>"#;

#[test]
fn declared_spill_without_stored_children_retains_extent_and_dynamic_metadata() {
    let mut model = Workbook::new();
    let sheet = model.add_sheet("Data").unwrap();
    let formula = "SEQUENCE(3,1,7)";
    sheet
        .set_formula_with_cached_value(at("A1"), formula, 7.0.into())
        .unwrap();
    sheet.set_spill_extent(at("A1"), at("A3"), formula).unwrap();

    let bytes = write_xlsx_to_buffer(&model).unwrap();
    let package = parts(&bytes);
    let xml = std::str::from_utf8(&package["xl/worksheets/sheet1.xml"]).unwrap();
    assert!(xml.contains("ref=\"A1:A3\""));
    assert!(xml.contains("cm=\"1\""));
    assert!(package.contains_key("xl/metadata.xml"));

    let restored = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    let sheet = restored.sheet(0).unwrap();
    assert_eq!(sheet.spill_extent(at("A1")), Some(at("A3")));
    assert_eq!(sheet.cells().len(), 1);
    assert_eq!(sheet.get(at("A1")).unwrap().value, CellValue::Number(7.0));
}

#[test]
fn fresh_save_preserves_calculation_data_and_omits_opaque_features() {
    let bytes = fixture(SHEET);
    let mut book = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_value(at("A1"), 10.).unwrap();
    sheet.set_value(at("D3"), true).unwrap();
    sheet.remove(at("A2"));
    sheet.get_mut(at("C1")).unwrap().value = CellValue::Number(33.);
    sheet.get_mut(at("C1")).unwrap().style.number_format = Some("0.000".into());
    let output = write_xlsx_to_buffer(&book).unwrap();
    let package = parts(&output);
    for path in [
        "xl/charts/chart1.xml",
        "xl/drawings/drawing1.xml",
        "xl/vbaProject.bin",
    ] {
        assert!(!package.contains_key(path));
    }
    let reopened = read_workbook_bytes::<poirot::model::Workbook>(&output).unwrap();
    let sheet = reopened.sheet(0).unwrap();
    assert_eq!(sheet.get(at("A1")).unwrap().value, CellValue::Number(10.));
    assert_eq!(
        sheet.get(at("B1")).unwrap().value,
        CellValue::Text("Rich text".into())
    );
    let formula = sheet.get(at("C1")).unwrap();
    assert_eq!(formula.formula.as_deref(), Some("1+2"));
    assert_eq!(formula.value, CellValue::Number(33.));
    assert_eq!(formula.style.number_format.as_deref(), Some("0.000"));
    assert_eq!(sheet.get(at("D3")).unwrap().value, CellValue::Boolean(true));
    assert!(sheet.get(at("A2")).is_none());
}

#[test]
fn imported_workbooks_allow_structural_edits_and_snapshot_roundtrip() {
    let mut book = read_workbook_bytes::<poirot::model::Workbook>(&fixture(SHEET)).unwrap();
    book.add_sheet("New")
        .unwrap()
        .set_value(at("A1"), 42.)
        .unwrap();
    book.date_system = poirot::model::DateSystem::Excel1904;
    let restored = poirot::model::snapshot::Snapshot::capture(&book)
        .into_owned()
        .restore()
        .unwrap();
    assert_eq!(book, restored);
    let reopened =
        read_workbook_bytes::<poirot::model::Workbook>(&write_xlsx_to_buffer(&restored).unwrap())
            .unwrap();
    assert_eq!(reopened.date_system, poirot::model::DateSystem::Excel1904);
    assert_eq!(reopened.sheets().len(), 2);
    assert_eq!(
        reopened.sheet(1).unwrap().get(at("A1")).unwrap().value,
        CellValue::Number(42.)
    );
}

#[test]
fn styled_only_blank_retains_presence_and_raw_format() {
    let mut source = Workbook::new();
    let sheet = source.add_sheet("Data").unwrap();
    let mut blank = poirot::model::Cell::default();
    blank.style.number_format = Some("0.000".into());
    sheet.set(at("B4"), blank).unwrap();
    let bytes = write_xlsx_to_buffer(&source).unwrap();
    let imported = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    let blank = imported.sheet(0).unwrap().get(at("B4")).unwrap();
    assert_eq!(blank.value, CellValue::Blank);
    assert_eq!(blank.style.number_format.as_deref(), Some("0.000"));
    let reopened =
        read_workbook_bytes::<poirot::model::Workbook>(&write_xlsx_to_buffer(&imported).unwrap())
            .unwrap();
    assert_eq!(reopened.sheet(0).unwrap().get(at("B4")), Some(blank));
}

#[test]
fn empty_physical_rows_remain_sparse_without_a_separate_row_scan() {
    let bytes = fixture(
        r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="2"/><row r="1000000"/></sheetData></worksheet>"#,
    );
    let book = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    assert_eq!(
        book.sheet(0).unwrap().row_indices().collect::<Vec<_>>(),
        vec![1, 999999]
    );
    assert!(book.sheet(0).unwrap().cells().is_empty());
    let reopened =
        read_workbook_bytes::<poirot::model::Workbook>(&write_xlsx_to_buffer(&book).unwrap())
            .unwrap();
    assert_eq!(
        reopened.sheet(0).unwrap().row_indices().collect::<Vec<_>>(),
        vec![1, 999999]
    );
}
