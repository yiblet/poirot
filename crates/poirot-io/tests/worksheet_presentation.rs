use poirot_io::{read_xlsx_bytes, write_xlsx_to_buffer};
use poirot_model::{SheetPresentation, SheetVisibility, Workbook};
use std::io::{Cursor, Read, Write};
fn parts(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut f = zip.by_index(i).unwrap();
            let name = f.name().to_owned();
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
    let report = read_xlsx_bytes(&bytes).unwrap();
    assert_eq!(
        report
            .workbook
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
        read_xlsx_bytes(&lower)
            .unwrap()
            .workbook
            .sheet(0)
            .unwrap()
            .presentation()
            .tab_color,
        Some([0x12, 0x34, 0xab])
    );
    // Each XML attribute combination has an independent expected loss diagnostic.
    for attributes in [
        "theme=\"4\" tint=\"0.5\"",
        "indexed=\"3\"",
        "auto=\"1\"",
        "rgb=\"FF1234AB\" tint=\"0.5\"",
        "rgb=\"FF1234AB\" theme=\"4\"",
        "rgb=\"001234AB\"",
        "rgb=\"invalid\"",
    ] {
        let changed = rewrite(&bytes, |name, b| {
            if name == "xl/worksheets/sheet1.xml" {
                String::from_utf8(b)
                    .unwrap()
                    .replace("rgb=\"FF1234AB\"", attributes)
                    .into_bytes()
            } else {
                b
            }
        });
        let report = read_xlsx_bytes(&changed).unwrap();
        assert_eq!(
            report.workbook.sheet(0).unwrap().presentation().tab_color,
            None,
            "{attributes}"
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|s| s.contains("Hidden: unsupported tab color")),
            "{attributes}"
        );
    }
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
    let report = read_xlsx_bytes(&changed).unwrap();
    assert!(
        report
            .workbook
            .sheets()
            .iter()
            .all(|s| s.presentation().visibility != SheetVisibility::Visible)
    );
    assert!(write_xlsx_to_buffer(&report.workbook).is_err());
}
