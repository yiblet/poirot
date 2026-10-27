//! Original synthetic regressions for the canonical XML projection around backend cells.
use poirot::model::{CellValue, Workbook};
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
use std::io::{Cursor, Read, Write};

fn package_with_sheet(sheet: &str) -> Vec<u8> {
    let mut book = Workbook::new();
    book.add_sheet("Data").unwrap();
    let source = write_xlsx_to_buffer(&book).unwrap();
    let mut source = zip::ZipArchive::new(Cursor::new(source)).unwrap();
    let mut target = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..source.len() {
        let mut part = source.by_index(index).unwrap();
        let name = part.name().unwrap().into_owned();
        let mut contents = Vec::new();
        part.read_to_end(&mut contents).unwrap();
        target
            .start_file(&name, zip::write::SimpleFileOptions::default())
            .unwrap();
        target
            .write_all(if name == "xl/worksheets/sheet1.xml" {
                sheet.as_bytes()
            } else {
                &contents
            })
            .unwrap();
    }
    target.finish().unwrap().into_inner()
}

#[test]
fn original_xml_restores_blank_cache_and_normalized_formula_without_backend_artifacts() {
    // XML 1.0 §2.11 normalizes CRLF and CR to LF. SpreadsheetML CT_Cell
    // keeps absent v distinct from a present empty string value.
    let bytes = package_with_sheet(
        "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData><row r=\"1\"><c r=\"A1\"/><c r=\"B1\" t=\"str\"><f>IF(1,\r\n\"x\",\"y\")</f></c><c r=\"C1\" t=\"str\"><f>\"\"</f><v></v></c><c r=\"D1\" t=\"inlineStr\"><is><t>_xD800_</t></is></c></row></sheetData></worksheet>",
    );
    let book = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    let sheet = book.sheet(0).unwrap();
    assert_eq!(sheet.get("A1").unwrap().unwrap().value, CellValue::Blank);
    let missing = sheet.get("B1").unwrap();
    assert_eq!(missing.unwrap().value, CellValue::Blank);
    assert_eq!(
        missing.unwrap().formula.as_deref(),
        Some("IF(1,\n\"x\",\"y\")")
    );
    assert_eq!(
        sheet
            .get("C1")
            .unwrap()
            .unwrap()
            .value
            .text_ref()
            .unwrap()
            .units()
            .collect::<Vec<_>>(),
        Vec::<u16>::new()
    );
    assert_eq!(
        sheet
            .get("D1")
            .unwrap()
            .unwrap()
            .value
            .text_ref()
            .unwrap()
            .units()
            .collect::<Vec<_>>(),
        vec![0xd800]
    );
    let restored =
        read_workbook_bytes::<poirot::model::Workbook>(&write_xlsx_to_buffer(&book).unwrap())
            .unwrap();
    assert_eq!(restored.sheet(0).unwrap(), sheet);
}

#[test]
fn empty_ordinary_formula_is_a_typed_error() {
    for formula in ["", " ", " = "] {
        let bytes = package_with_sheet(&format!(
            "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData><row r=\"1\"><c r=\"A1\"><f>{formula}</f><v>1</v></c></row></sheetData></worksheet>"
        ));
        assert!(read_workbook_bytes::<poirot::model::Workbook>(&bytes).is_err());
    }
}

#[test]
fn ordinary_formula_uses_model_source_normalization() {
    let bytes = package_with_sheet(
        "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData><row r=\"1\"><c r=\"A1\"><f> = 1+2 </f><v>3</v></c></row></sheetData></worksheet>",
    );
    let book = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    assert_eq!(
        book.sheet(0)
            .unwrap()
            .get("A1")
            .unwrap()
            .unwrap()
            .formula
            .as_deref(),
        Some("1+2")
    );
}
