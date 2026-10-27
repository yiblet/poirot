//! Public style workflows use handwritten OOXML and independently specified properties.
use poirot::calculation::EvaluationContext;
use poirot::eval::EvaluationTarget;
use poirot::styles::{
    CellStyle, ColorSource, FontStyle, HorizontalAlignment, StyleColor, VisualStyle,
};
use poirot::{CellValue, Workbook};
use std::{
    io::{Cursor, Read, Write},
    sync::Arc,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

/// Address a cell in the single-sheet acceptance model.
fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

/// Create a tiny source package independently of Poirot's exporter.
fn imported_book() -> Workbook {
    // CellFormat defines component references; color retains its theme slot:
    // https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.cellformat
    // https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.color
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, xml) in [
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        ),
        (
            "xl/workbook.xml",
            r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Inputs" sheetId="1" r:id="sheet"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="sheet" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="style" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#,
        ),
        (
            "xl/styles.xml",
            r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts><font/><font><b/><color theme="4"/></font></fonts><cellXfs><xf/><xf fontId="1" numFmtId="2"><alignment horizontal="right" wrapText="1"/></xf></cellXfs></styleSheet>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>3</v></c><c r="B1" s="1"><f>A1*2</f><v>6</v></c><c r="C1" s="1"/></row></sheetData></worksheet>"#,
        ),
    ] {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    Workbook::from_bytes(&zip.finish().unwrap().into_inner()).unwrap()
}

/// Inspect output XML without asking the importer to validate its own writer.
fn part(bytes: &[u8], name: &str) -> String {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    zip.by_name(name)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

#[test]
fn imported_styles_reach_evaluation_and_survive_edits_reset_flush_and_snapshots() {
    let book = imported_book();
    let original = book
        .sheet(0)
        .unwrap()
        .get(at("B1").address)
        .unwrap()
        .style
        .clone();
    assert_eq!(original.number_format.as_deref(), Some("0.00"));
    let visual = original.visual.as_ref().unwrap();
    let font = visual.font.as_ref().unwrap();
    assert!(font.bold);
    assert_eq!(
        font.color.unwrap().source,
        ColorSource::Theme(poirot::styles::ThemeColor::Accent1)
    );
    assert_eq!(
        visual.alignment.as_ref().unwrap().horizontal,
        Some(HorizontalAlignment::Right)
    );
    assert!(visual.alignment.as_ref().unwrap().wrap_text);
    assert!(Arc::ptr_eq(
        visual,
        book.sheet(0)
            .unwrap()
            .get(at("C1").address)
            .unwrap()
            .style
            .visual
            .as_ref()
            .unwrap()
    ));
    let source = book.save_snapshot().to_buffer().unwrap();
    assert_eq!(
        Workbook::from_model(
            Workbook::restore_snapshot()
                .from_bytes(&source)
                .unwrap()
                .into_artifact()
                .into_parts()
                .0
        ),
        book
    );

    let context = EvaluationContext::default();
    let mut overlay = poirot::calculation::Overlay::from_overlay(
        poirot::eval::overlay::Overlay::with_workbook(book.model().clone())
            .build()
            .unwrap(),
    );
    let edited = CellStyle {
        visual: Some(Arc::new(VisualStyle {
            font: Some(FontStyle {
                italic: true,
                color: Some(StyleColor {
                    source: ColorSource::Argb([255, 10, 20, 30]),
                    tint: None,
                }),
                ..Default::default()
            }),
            ..Default::default()
        })),
        ..Default::default()
    };
    overlay.set_style(at("B1"), edited.clone()).unwrap();
    overlay.set_value(at("A1"), 4.).unwrap();
    let output = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&[at("B1")])
        .unwrap()
        .remove(0);
    assert_eq!(output.value, CellValue::Number(8.));
    // A visual-only style must remain visible in evaluation presentation metadata.
    assert_eq!(output.formatting, Some(edited.clone()));
    overlay.discard_edits();
    let output = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&[at("B1")])
        .unwrap()
        .remove(0);
    assert_eq!(output.value, CellValue::Number(6.));
    assert_eq!(output.formatting, Some(original.clone()));

    overlay.set_value(at("A1"), 5.).unwrap();
    overlay.set_style(at("B1"), edited.clone()).unwrap();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("B1"))
        .unwrap();
    let checkpoint = overlay.save_snapshot().to_buffer().unwrap();
    let mut restored = Workbook::restore_snapshot()
        .from_bytes(&checkpoint)
        .unwrap()
        .into_evaluation(context.clone())
        .unwrap();
    let output = restored.evaluation.outputs(&[at("B1")]).unwrap().remove(0);
    assert_eq!(output.value, CellValue::Number(10.));
    assert_eq!(output.formatting, Some(edited.clone()));
    assert_eq!(overlay.apply_edits_to_workbook().unwrap(), 2);
    let bytes = overlay.save_xlsx().to_buffer().unwrap();
    let styles = part(&bytes, "xl/styles.xml");
    assert!(styles.contains(r#"<color rgb="FF0A141E"/>"#));
    assert!(styles.contains("<i/>"));
    let cells = part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(cells.contains("<f>A1*2</f><v>10</v>"));
    let reopened = Workbook::from_bytes(&bytes).unwrap();
    assert_eq!(
        reopened
            .sheet(0)
            .unwrap()
            .get(at("B1").address)
            .unwrap()
            .style,
        edited
    );
    assert_eq!(
        reopened
            .sheet(0)
            .unwrap()
            .get(at("C1").address)
            .unwrap()
            .style,
        original
    );
    assert_eq!(
        reopened
            .sheet(0)
            .unwrap()
            .get(at("C1").address)
            .unwrap()
            .value,
        CellValue::Blank
    );
    assert_eq!(
        book.sheet(0).unwrap().get(at("B1").address).unwrap().value,
        CellValue::Number(6.)
    );
}
