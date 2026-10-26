use poirot::model::{SheetPresentation, SheetVisibility, Workbook};
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
use std::io::{Cursor, Read, Write};
fn parts(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut f = zip.by_index(i).unwrap();
            let name = f.name().unwrap().into_owned();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            (name, b)
        })
        .collect()
}
fn rewrite(bytes: &[u8], replace: impl Fn(&str, Vec<u8>) -> Vec<u8>) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, b) in parts(bytes) {
        let b = replace(&name, b);
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&b).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
#[test]
fn independent_xml_and_reload_preserve_first_hidden_very_hidden_and_rgb() {
    let mut w = Workbook::new();
    for (name, visibility, color) in [
        ("Hidden", SheetVisibility::Hidden, Some([0x12, 0x34, 0xab])),
        ("Visible", SheetVisibility::Visible, None),
        (
            "Restricted",
            SheetVisibility::VeryHidden,
            Some([255, 0, 127]),
        ),
    ] {
        w.add_sheet(name)
            .unwrap()
            .set_presentation(SheetPresentation {
                visibility,
                tab_color: color,
            });
    }
    let bytes = write_xlsx_to_buffer(&w).unwrap();
    let package = parts(&bytes);
    let xml = String::from_utf8(
        package
            .iter()
            .find(|(n, _)| n == "xl/workbook.xml")
            .unwrap()
            .1
            .clone(),
    )
    .unwrap();
    assert!(
        xml.contains("name=\"Hidden\" sheetId=\"1\" state=\"hidden\""),
        "{xml}"
    );
    assert!(xml.contains("state=\"veryHidden\""), "{xml}");
    let xml = String::from_utf8(
        package
            .iter()
            .find(|(n, _)| n == "xl/worksheets/sheet1.xml")
            .unwrap()
            .1
            .clone(),
    )
    .unwrap();
    assert!(xml.contains("<tabColor rgb=\"FF1234AB\">"), "{xml}");
    let report = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    assert_eq!(
        report
            .sheets()
            .iter()
            .map(|s| s.presentation())
            .collect::<Vec<_>>(),
        w.sheets()
            .iter()
            .map(|s| s.presentation())
            .collect::<Vec<_>>()
    );
    let lower = rewrite(&bytes, |name, b| {
        if name == "xl/worksheets/sheet1.xml" {
            String::from_utf8(b)
                .unwrap()
                .replace("FF1234AB", "ff1234ab")
                .into_bytes()
        } else {
            b
        }
    });
    assert_eq!(
        read_workbook_bytes::<poirot::model::Workbook>(&lower)
            .unwrap()
            .sheet(0)
            .unwrap()
            .presentation()
            .tab_color,
        Some([0x12, 0x34, 0xab])
    );
    // Non-RGB color resolution follows rxls; exact source styling is not retained.
    // Imports preserve all-hidden source; exporter refuses rather than exposing it.
    let changed = rewrite(&bytes, |name, b| {
        if name == "xl/workbook.xml" {
            String::from_utf8(b)
                .unwrap()
                .replace(
                    "name=\"Visible\" sheetId=\"2\"",
                    "name=\"Visible\" state=\"hidden\" sheetId=\"2\"",
                )
                .into_bytes()
        } else {
            b
        }
    });
    let report = read_workbook_bytes::<poirot::model::Workbook>(&changed).unwrap();
    assert!(
        report
            .sheets()
            .iter()
            .all(|s| s.presentation().visibility != SheetVisibility::Visible)
    );
    assert!(write_xlsx_to_buffer(&report).is_err());
}

#[test]
fn prefixed_presentation_and_cell_styles_use_backend_metadata() {
    // CT_Worksheet orders sheetPr before sheetData (ECMA-376 Part 1 §18.3.1.99).
    let mut book = Workbook::new();
    for color in [Some([18, 52, 86]), None] {
        let name = format!("Sheet{}", book.sheets().len());
        let sheet = book.add_sheet(name).unwrap();
        sheet.set_presentation(SheetPresentation {
            visibility: SheetVisibility::Visible,
            tab_color: color,
        });
        let address = "A1".parse().unwrap();
        sheet
            .set_formula_with_cached_value(address, "1+2", 3.0.into())
            .unwrap();
        sheet.get_mut(address).unwrap().style.number_format = Some("0.00".into());
    }
    let bytes = write_xlsx_to_buffer(&book).unwrap();
    let prefixed = rewrite(&bytes, |name, bytes| {
        if !name.starts_with("xl/worksheets/") {
            return bytes;
        }
        let mut source = String::from_utf8(bytes).unwrap().replace(
            "<worksheet ",
            "<worksheet xmlns:m=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" ",
        );
        for tag in ["sheetPr", "tabColor", "sheetData"] {
            source = source
                .replace(&format!("<{tag}"), &format!("<m:{tag}"))
                .replace(&format!("</{tag}>"), &format!("</m:{tag}>"));
        }
        source.into_bytes()
    });
    let loaded = read_workbook_bytes::<poirot::model::Workbook>(&prefixed).unwrap();
    for (original, actual) in book.sheets().iter().zip(loaded.sheets()) {
        assert_eq!(actual.presentation(), original.presentation());
        assert_eq!(
            actual.get("A1".parse().unwrap()),
            original.get("A1".parse().unwrap())
        );
    }
}
