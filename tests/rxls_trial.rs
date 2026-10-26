//! Compatibility probes for the pinned rxls 0.1.4 experiment.
//! Fixtures are authored here, not copied from rxls. Expected values follow
//! SpreadsheetML cell types and shared-formula rules:
//! https://learn.microsoft.com/en-us/office/open-xml/spreadsheet/working-with-formulas
//! Known gaps below characterize rxls; they are not Poirot regressions.
use poirot::{CellValue, Workbook};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};

fn parts(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut parts = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let name = entry.name().unwrap().into_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        parts.insert(name, bytes);
    }
    parts
}

fn package(parts: BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn fixture(rows: &str) -> Vec<u8> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Data").unwrap();
    let mut parts = parts(&poirot_io::write_xlsx_to_buffer(workbook.model()).unwrap());
    parts.insert("xl/worksheets/sheet1.xml".into(), format!(r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:probe="urn:poirot:io-trial"><sheetData>{rows}</sheetData><extLst><ext uri="urn:poirot:io-trial"><probe:payload preserved="yes">opaque &amp; untouched</probe:payload></ext></extLst></worksheet>"#).into_bytes());
    let types = String::from_utf8(parts.remove("[Content_Types].xml").unwrap()).unwrap();
    parts.insert("[Content_Types].xml".into(), types.replace("</Types>", r#"<Override PartName="/custom/trial.bin" ContentType="application/octet-stream"/></Types>"#).into_bytes());
    parts.insert("custom/trial.bin".into(), vec![0, 1, 2, 255]);
    package(parts)
}

#[test]
fn retained_noop_and_edits_preserve_opaque_parts_and_worksheet_extensions() {
    let source = fixture(
        r#"<row r="1"><c r="A1" s="0"><v>17</v></c><c r="B1"><f>A1*2</f><v>34</v></c><c r="C1" t="e"><v>#SPILL!</v></c></row>"#,
    );
    let before = parts(&source);
    let mut spreadsheet = rxls::Spreadsheet::open(&source).unwrap();
    assert_eq!(
        spreadsheet.edit_capability(),
        &rxls::EditCapability::ReadWrite
    );
    assert_eq!(parts(&spreadsheet.save().unwrap()), before);
    spreadsheet
        .set_cell_value("Data", 0, 0, rxls::Cell::Number(123.5))
        .unwrap();
    spreadsheet
        .set_cell_formula("Data", 0, 1, "1+2", rxls::Cell::Number(3.))
        .unwrap();
    let saved = spreadsheet.save().unwrap();
    let after = parts(&saved);
    for (name, bytes) in &before {
        if name != "xl/worksheets/sheet1.xml" {
            assert_eq!(after.get(name), Some(bytes), "unexpected change to {name}");
        }
    }
    let xml = std::str::from_utf8(after.get("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(
        xml.contains(r#"<probe:payload preserved="yes">opaque &amp; untouched</probe:payload>"#)
    );
    let reopened = Workbook::from_bytes(&saved).unwrap();
    let sheet = reopened.sheet(0).unwrap();
    assert_eq!(
        sheet.get("A1".parse().unwrap()).unwrap().value,
        CellValue::Number(123.5)
    );
    let cell = sheet.get("B1".parse().unwrap()).unwrap();
    assert_eq!(cell.formula.as_deref(), Some("1+2"));
    assert_eq!(cell.value, CellValue::Number(3.));
    assert_eq!(
        sheet.get("C1".parse().unwrap()).unwrap().value,
        CellValue::Error(poirot::CellError::Spill)
    );
}

#[test]
fn ordinary_types_and_shared_formula_translation_are_available() {
    let source = fixture(
        r#"<row r="1"><c r="A1"><v>17</v></c><c r="B1" t="b"><v>1</v></c><c r="C1" t="e"><v>#CALC!</v></c><c r="D1" t="inlineStr"><is><t>A&amp;B</t></is></c><c r="E1"><f t="shared" si="0" ref="E1:E2">A1+$B$1</f><v>18</v></c></row><row r="2"><c r="E2"><f t="shared" si="0"/><v>1</v></c></row>"#,
    );
    let workbook = rxls::Workbook::open(&source).unwrap();
    let sheet = workbook.sheet_by_name("Data").unwrap();
    assert_eq!(sheet.cell(0, 0), Some(&rxls::Cell::Number(17.)));
    assert_eq!(sheet.cell(0, 1), Some(&rxls::Cell::Bool(true)));
    assert_eq!(sheet.cell(0, 2), Some(&rxls::Cell::Error("#CALC!".into())));
    assert_eq!(sheet.cell(0, 3), Some(&rxls::Cell::Text("A&B".into())));
    assert_eq!(
        sheet.cell(1, 4),
        Some(&rxls::Cell::Formula {
            formula: "A2+$B$1".into(),
            cached: Box::new(rxls::Cell::Number(1.))
        })
    );
}

#[test]
fn rxls_014_retains_raw_escapes_but_does_not_decode_excel_text_units() {
    // ST_Xstring escapes encode UTF-16 units. An escaped underscore makes
    // the following escape-looking sequence literal, not a surrogate.
    // https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/bd0aa042-434a-4ca7-b25f-4e1fd25a954d
    let source = fixture(
        r#"<row r="1"><c r="A1" t="inlineStr"><is><t>_xD83D__xDE00_</t></is></c><c r="B1" t="inlineStr"><is><t>_xD83D_</t></is></c><c r="C1" t="inlineStr"><is><t>_x005F_xD83D_</t></is></c></row>"#,
    );
    let poirot = Workbook::from_bytes(&source).unwrap();
    for (col, expected) in [
        (0, vec![0xd83d, 0xde00]),
        (1, vec![0xd83d]),
        (2, "_xD83D_".encode_utf16().collect()),
    ] {
        assert_eq!(
            poirot
                .sheet(0)
                .unwrap()
                .get(poirot::CellAddress::new(0, col).unwrap())
                .unwrap()
                .value
                .text_ref()
                .unwrap()
                .units()
                .collect::<Vec<_>>(),
            expected
        );
    }
    let rxls = rxls::Spreadsheet::open(&source).unwrap();
    let sheet = rxls.workbook().sheet_by_name("Data").unwrap();
    for (col, raw) in [(0, "_xD83D__xDE00_"), (1, "_xD83D_"), (2, "_x005F_xD83D_")] {
        assert_eq!(sheet.cell(0, col), Some(&rxls::Cell::Text(raw.into())));
    }
    assert_eq!(parts(&rxls.save().unwrap()), parts(&source));
}

#[test]
fn rxls_014_conflates_missing_formula_cache_with_empty_text_and_omits_blanks() {
    let source = fixture(
        r#"<row r="1"><c r="A1"><f>1</f></c><c r="B1" t="str"><f>1</f><v/></c><c r="C1" t="str"><v/></c><c r="D1" s="0"/></row><row r="3"/>"#,
    );
    let poirot = Workbook::from_bytes(&source).unwrap();
    let sheet = poirot.sheet(0).unwrap();
    assert_eq!(
        sheet.get("A1".parse().unwrap()).unwrap().value,
        CellValue::Blank
    );
    assert!(
        sheet
            .get("B1".parse().unwrap())
            .unwrap()
            .value
            .text_ref()
            .unwrap()
            .is_empty()
    );
    let workbook = rxls::Workbook::open(&source).unwrap();
    let sheet = workbook.sheet_by_name("Data").unwrap();
    assert_eq!(sheet.cell(0, 0), sheet.cell(0, 1));
    assert_eq!(sheet.cell(0, 2), None);
    assert_eq!(sheet.cell(0, 3), None);
}

#[test]
fn failed_edit_leaves_retained_package_unchanged() {
    let source = fixture(r#"<row r="1"><c r="A1"><v>17</v></c></row>"#);
    let mut spreadsheet = rxls::Spreadsheet::open(&source).unwrap();
    let before = spreadsheet.save().unwrap();
    assert!(
        spreadsheet
            .set_cell_value("Data", 1_048_576, 0, rxls::Cell::Number(1.))
            .is_err()
    );
    assert_eq!(spreadsheet.save().unwrap(), before);
    assert!(spreadsheet.edited_parts().is_empty());
    assert!(rxls::Spreadsheet::open(b"not a workbook").is_err());
}

#[test]
fn rxls_014_keeps_crlf_instead_of_xml_line_end_normalization() {
    // XML 1.0 section 2.11 requires CRLF to become LF before application parsing.
    // https://www.w3.org/TR/REC-xml/#sec-line-ends
    let source = fixture(
        "<row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>a\r\nb</t></is></c><c r=\"B1\"><f>1+\r\n2</f><v>3</v></c></row>",
    );
    let poirot = Workbook::from_bytes(&source).unwrap();
    assert_eq!(
        poirot
            .sheet(0)
            .unwrap()
            .get("A1".parse().unwrap())
            .unwrap()
            .value
            .text_ref()
            .unwrap()
            .units()
            .collect::<Vec<_>>(),
        "a\nb".encode_utf16().collect::<Vec<_>>()
    );
    let rxls = rxls::Workbook::open(&source).unwrap();
    let sheet = rxls.sheet_by_name("Data").unwrap();
    assert_eq!(sheet.cell(0, 0), Some(&rxls::Cell::Text("a\r\nb".into())));
    assert_eq!(
        sheet.cell(0, 1),
        Some(&rxls::Cell::Formula {
            formula: "1+\r\n2".into(),
            cached: Box::new(rxls::Cell::Number(3.))
        })
    );
}

#[test]
fn rxls_014_single_cell_edit_can_orphan_shared_formula_followers() {
    let source = fixture(
        r#"<row r="1"><c r="A1"><v>17</v></c><c r="B1"><f t="shared" si="0" ref="B1:B2">A1+1</f><v>18</v></c></row><row r="2"><c r="B2"><f t="shared" si="0"/><v>1</v></c></row>"#,
    );
    let mut spreadsheet = rxls::Spreadsheet::open(&source).unwrap();
    assert!(
        matches!(spreadsheet.workbook().sheet_by_name("Data").unwrap().cell(1,1), Some(rxls::Cell::Formula { formula, .. }) if formula == "A2+1")
    );
    spreadsheet
        .set_cell_formula("Data", 0, 1, "99", rxls::Cell::Number(99.))
        .unwrap();
    let saved = spreadsheet.save().unwrap();
    let reopened = rxls::Workbook::open(&saved).unwrap();
    // Known incompatibility: B2 still carries si=0, but its template was removed.
    // A compatible editor must materialize/rebase the group or reject this edit.
    assert_eq!(
        reopened.sheet_by_name("Data").unwrap().cell(1, 1),
        Some(&rxls::Cell::Number(1.))
    );
    let after = parts(&saved);
    let xml = std::str::from_utf8(after.get("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(xml.contains(r#"<f t="shared" si="0"/>"#));
}

#[test]
fn rxls_014_array_children_are_reported_as_formulas_without_spill_ownership() {
    let source = fixture(
        r#"<row r="1"><c r="A1"><f t="array" ref="A1:B1">{1,2}</f><v>1</v></c><c r="B1"><v>2</v></c></row>"#,
    );
    let poirot = Workbook::from_bytes(&source).unwrap();
    let sheet = poirot.sheet(0).unwrap();
    assert_eq!(
        sheet.spill_owner("B1".parse().unwrap()),
        Some("A1".parse().unwrap())
    );
    assert_eq!(sheet.get("B1".parse().unwrap()).unwrap().formula, None);
    let rxls = rxls::Workbook::open(&source).unwrap();
    assert_eq!(
        rxls.sheet_by_name("Data").unwrap().cell(0, 1),
        Some(&rxls::Cell::Formula {
            formula: "{1,2}".into(),
            cached: Box::new(rxls::Cell::Number(2.))
        })
    );
}
