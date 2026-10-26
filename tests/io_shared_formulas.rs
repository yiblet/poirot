//! Original fixtures for SpreadsheetML CT_CellFormula shared groups.
//! ECMA-376 Part 1 §18.3.1.40: si identifies actual shared members; relative
//! references translate from the master while dollar-anchored axes remain fixed.
use poirot::model::{CellAddress, CellValue, SharedFormulaId, Workbook};
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
use std::io::{Cursor, Read, Write};

fn at(text: &str) -> CellAddress {
    text.parse().unwrap()
}
fn package(sheets: &[&str]) -> Vec<u8> {
    package_xml(&sheets.iter().map(|source| format!("<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>{source}</sheetData></worksheet>")).collect::<Vec<_>>())
}
fn package_xml(sheets: &[String]) -> Vec<u8> {
    let mut book = Workbook::new();
    for n in 0..sheets.len() {
        book.add_sheet(format!("Sheet{n}")).unwrap();
    }
    let bytes = write_xlsx_to_buffer(&book).unwrap();
    let mut input = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..input.len() {
        let mut part = input.by_index(i).unwrap();
        let name = part.name().unwrap().into_owned();
        let mut bytes = Vec::new();
        part.read_to_end(&mut bytes).unwrap();
        for (index, source) in sheets.iter().enumerate() {
            if name == format!("xl/worksheets/sheet{}.xml", index + 1) {
                bytes = source.as_bytes().to_vec();
            }
        }
        output
            .start_file(&name, zip::write::SimpleFileOptions::default())
            .unwrap();
        output.write_all(&bytes).unwrap();
    }
    output.finish().unwrap().into_inner()
}
#[test]
fn shared_membership_and_relative_absolute_sources() {
    let bytes = package(&[
        r#"<row r="1"><c r="B1"><f t="shared" si="4294967295" ref="B1:C2">A1+$B2+C$3+$D$4+LEN("A1")</f><v>7</v></c></row><row r="2"><c r="C2"><f t="shared" si="4294967295"/><v>8</v></c></row>"#,
    ]);
    let book = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    let sheet = book.sheet(0).unwrap();
    let anchor = sheet.get(at("B1")).unwrap();
    let follower = sheet.get(at("C2")).unwrap();
    assert_eq!(
        anchor.formula.as_deref(),
        Some("A1+$B2+C$3+$D$4+LEN(\"A1\")")
    );
    assert_eq!(
        follower.formula.as_deref(),
        Some("B2+$B3+D$3+$D$4+LEN(\"A1\")")
    );
    assert_eq!(follower.value, CellValue::Number(8.));
    assert!(anchor.shared_formula.is_none());
    assert!(follower.shared_formula.is_none());
}
#[test]
fn group_ids_are_worksheet_local() {
    let book = read_workbook_bytes::<poirot::model::Workbook>(&package(&[
        r#"<row><c><f t="shared" si="0" ref="A1:B1">1</f></c><c><f t="shared" si="0"/></c></row>"#,
        r#"<row><c><f t="shared" si="0" ref="A1:B1">2</f></c><c><f t="shared" si="0"/></c></row>"#,
    ]))
    .unwrap();
    for (index, source) in [(0, "1"), (1, "2")] {
        let cell = book.sheet(index).unwrap().get(at("B1")).unwrap();
        assert_eq!(cell.formula.as_deref(), Some(source));
        assert!(cell.shared_formula.is_none());
        assert_eq!(cell.value, CellValue::Blank);
    }
}
#[test]
fn independent_holes_are_not_members_and_overlapping_bounds_are_recovered() {
    let book = read_workbook_bytes::<poirot::model::Workbook>(&package(&[r#"<row r="1"><c r="A1"><f t="shared" si="0" ref="A1:D2">1</f><v>1</v></c><c r="B1"><f>UNKNOWN_VENDOR_FN(1)</f><v>2</v></c><c r="C1"><v>3</v></c><c r="D1"><f t="shared" si="1" ref="C1:D2">4</f><v>4</v></c></row><row r="2"><c r="A2"><f t="shared" si="0"/></c><c r="D2"><f t="shared" si="1"/></c></row>"#])).unwrap();
    let sheet = book.sheet(0).unwrap();
    assert_eq!(sheet.cells().len(), 6);
    assert!(sheet.get(at("B1")).unwrap().shared_formula.is_none());
    assert_eq!(
        sheet.get(at("B1")).unwrap().formula.as_deref(),
        Some("UNKNOWN_VENDOR_FN(1)")
    );
    assert!(sheet.get(at("C1")).unwrap().shared_formula.is_none());
    assert_eq!(sheet.get(at("C1")).unwrap().value, CellValue::Number(3.));
    assert!(sheet.get(at("D2")).unwrap().shared_formula.is_none());
    assert_eq!(sheet.get(at("D2")).unwrap().formula.as_deref(), Some("4"));
}
#[test]
fn forward_shared_followers_translate_from_actual_master() {
    // The existing text pass repairs the backend's unresolved forward follower.
    let bytes = package(&[
        r#"<row><c><f t="shared" si="3"/><v>11</v></c></row><row><c r="B2"><f t="shared" si="3" ref="A1:B2">B2+$C3+D$4+$E$5</f><v>22</v></c></row>"#,
    ]);
    let book = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    assert_eq!(
        book.sheet(0)
            .unwrap()
            .get(at("A1"))
            .unwrap()
            .formula
            .as_deref(),
        Some("A1+$C2+C$4+$E$5")
    );
}
#[test]
fn shared_and_array_formulas_remain_distinct() {
    let book = read_workbook_bytes::<poirot::model::Workbook>(&package(&[r#"<row><c r="A1"><f t="shared" si="0" ref="A1:B1">1</f><v>1</v></c><c r="B1"><f t="shared" si="0"/><v>1</v></c><c r="D1"><f t="array" ref="D1:E1">{2,3}</f><v>2</v></c><c r="E1"><v>3</v></c></row>"#])).unwrap();
    let sheet = book.sheet(0).unwrap();
    assert!(sheet.get(at("B1")).unwrap().shared_formula.is_none());
    assert!(sheet.spill_owner(at("B1")).is_none());
    assert!(sheet.get(at("D1")).unwrap().shared_formula.is_none());
    assert_eq!(sheet.spill_owner(at("E1")), Some(at("D1")));
}
#[test]
fn backend_shared_structured_source_and_ordinary_source_survive() {
    let source = r#"<row><c><f t="shared" si="0" ref="A1:B1">Table1[Column]</f></c><c><f t="shared" si="0"/></c></row>"#;
    let shared = read_workbook_bytes::<poirot::model::Workbook>(&package(&[source])).unwrap();
    for coordinate in ["A1", "B1"] {
        assert_eq!(
            shared
                .sheet(0)
                .unwrap()
                .get(at(coordinate))
                .unwrap()
                .formula
                .as_deref(),
            Some("Table1[Column]")
        );
    }
    let ordinary = r#"<row><c><f>Table1[Column]</f></c></row>"#;
    let book = read_workbook_bytes::<poirot::model::Workbook>(&package(&[ordinary])).unwrap();
    assert_eq!(
        book.sheet(0)
            .unwrap()
            .get(at("A1"))
            .unwrap()
            .formula
            .as_deref(),
        Some("Table1[Column]")
    );
}

#[test]
fn nonempty_shared_follower_expression_is_ignored_in_favor_of_master() {
    let bytes = package(&[
        r#"<row><c><f t="shared" si="0" ref="A1:C1">A2</f></c><c><f t="shared" si="0">99</f></c><c><f t="shared" si="0"/></c></row>"#,
    ]);
    let book = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    assert_eq!(
        book.sheet(0)
            .unwrap()
            .get(at("B1"))
            .unwrap()
            .formula
            .as_deref(),
        Some("B2")
    );
    assert_eq!(
        book.sheet(0)
            .unwrap()
            .get(at("C1"))
            .unwrap()
            .formula
            .as_deref(),
        Some("C2")
    );
}

#[test]
fn forward_shared_follower_cannot_be_claimed_as_an_array_cache() {
    let bytes = package(&[
        r#"<row><c r="A1"><f t="array" ref="A1:B1">{1,2}</f><v>1</v></c><c r="B1"><f t="shared" si="0"/><v>2</v></c></row><row><c r="B2"><f t="shared" si="0" ref="B1:B2">2</f><v>2</v></c></row>"#,
    ]);
    assert!(read_workbook_bytes::<poirot::model::Workbook>(&bytes).is_err());
}

#[test]
fn extension_formula_outside_cells_does_not_join_or_break_shared_groups() {
    let bytes = package_xml(&[r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" xmlns:xm="http://schemas.microsoft.com/office/excel/2006/main"><sheetData><row><c><f t="shared" si="0" ref="A1:B1">1</f></c><c><f t="shared" si="0"/></c></row></sheetData><extLst><ext uri="{05C60535-1F16-4fd2-B633-F4F36F0B64E0}"><x14:sparklineGroups><x14:sparklineGroup><x14:sparklines><x14:sparkline><xm:f>Sheet0!A1:B1</xm:f><xm:sqref>D1</xm:sqref></x14:sparkline></x14:sparklines></x14:sparklineGroup></x14:sparklineGroups></ext></extLst></worksheet>"#.into()]);
    let book = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    assert_eq!(book.sheet(0).unwrap().cells().len(), 2);
    assert_eq!(
        book.sheet(0)
            .unwrap()
            .get(at("B1"))
            .unwrap()
            .formula
            .as_deref(),
        Some("1")
    );
}

fn exported_xml(book: &Workbook) -> (Vec<u8>, String) {
    let bytes = write_xlsx_to_buffer(book).unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("xl/worksheets/sheet1.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    (bytes, xml)
}
fn mark_group(book: &mut Workbook, coordinates: &[&str], origin: &str) {
    for coordinate in coordinates {
        book.sheet_mut(0)
            .unwrap()
            .get_mut(at(coordinate))
            .unwrap()
            .shared_formula = Some(poirot::model::SharedFormula {
            group: SharedFormulaId(17),
            origin: at(origin),
        });
    }
}
fn vertical_group() -> Workbook {
    let mut book = read_workbook_bytes::<poirot::model::Workbook>(&package(&[r#"<row r="1"><c r="B1"><f t="shared" si="17" ref="B1:B3">A1+1</f><v>11</v></c></row><row r="2"><c r="B2"><f t="shared" si="17"/><v>22</v></c></row><row r="3"><c r="B3"><f t="shared" si="17"/><v>33</v></c></row>"#])).unwrap();
    mark_group(&mut book, &["B1", "B2", "B3"], "B1");
    book
}
#[test]
fn deleting_or_replacing_original_anchor_promotes_a_live_master() {
    for replace in [false, true] {
        let mut book = vertical_group();
        let sheet = book.sheet_mut(0).unwrap();
        if replace {
            sheet.set_formula(at("B1"), "99").unwrap();
        } else {
            sheet.remove(at("B1"));
        }
        let original = book.clone();
        let (bytes, xml) = exported_xml(&book);
        assert!(
            xml.contains(r#"<f t="shared" si="0" ref="B2:B3">A2+1</f>"#),
            "{xml}"
        );
        assert!(xml.contains(r#"<f t="shared" si="0"/>"#));
        // Saving is an immutable projection; original group provenance is retained.
        assert_eq!(book, original);
        let reopened = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
        for (coordinate, source, cache) in [("B2", "A2+1", 22.), ("B3", "A3+1", 33.)] {
            let cell = reopened.sheet(0).unwrap().get(at(coordinate)).unwrap();
            assert_eq!(cell.formula.as_deref(), Some(source));
            assert_eq!(cell.value, CellValue::Number(cache));
        }
        assert_eq!(exported_xml(&book).1, xml);
    }
}
#[test]
fn follower_edits_clears_singletons_and_empty_groups_are_independent() {
    let mut book = vertical_group();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("B2"), "42")
        .unwrap();
    let (bytes, xml) = exported_xml(&book);
    assert!(xml.contains(r#"ref="B1:B3">A1+1</f>"#));
    assert!(xml.contains("<f>42</f>"));
    let reopened = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    assert_eq!(
        reopened
            .sheet(0)
            .unwrap()
            .get(at("B2"))
            .unwrap()
            .formula
            .as_deref(),
        Some("42")
    );
    book.sheet_mut(0)
        .unwrap()
        .set_value(at("B3"), 123.)
        .unwrap();
    let (bytes, xml) = exported_xml(&book);
    assert!(!xml.contains(r#"t="shared""#));
    let reopened = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    assert!(
        reopened
            .sheet(0)
            .unwrap()
            .get(at("B1"))
            .unwrap()
            .shared_formula
            .is_none()
    );
    book.sheet_mut(0).unwrap().remove(at("B1"));
    let (_, xml) = exported_xml(&book);
    assert!(!xml.contains(r#"t="shared""#));
}
#[test]
fn stale_or_unsupported_sources_fall_back_without_losing_formulas() {
    for replacement in ["99", "Table1[Column]", "UNKNOWN_VENDOR_FN(A2)"] {
        let mut book = vertical_group();
        book.sheet_mut(0)
            .unwrap()
            .get_mut(at("B2"))
            .unwrap()
            .formula = Some(replacement.into());
        let (bytes, xml) = exported_xml(&book);
        assert!(!xml.contains(r#"t="shared""#), "{xml}");
        let reopened = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
        for (coordinate, source) in [("B1", "A1+1"), ("B2", replacement), ("B3", "A3+1")] {
            assert_eq!(
                reopened
                    .sheet(0)
                    .unwrap()
                    .get(at(coordinate))
                    .unwrap()
                    .formula
                    .as_deref(),
                Some(source)
            );
        }
    }
}
#[test]
fn verified_mixed_references_string_literals_cache_and_style_survive() {
    let mut book = read_workbook_bytes::<poirot::model::Workbook>(&package(&[r#"<row r="1"><c r="B1"><f t="shared" si="3" ref="B1:C2">A1+$B2+C$3+$D$4+LEN("A1")</f><v>7</v></c></row><row r="2"><c r="C2"><f t="shared" si="3"/><v>8</v></c></row>"#])).unwrap();
    mark_group(&mut book, &["B1", "C2"], "B1");
    let follower = book.sheet_mut(0).unwrap().get_mut(at("C2")).unwrap();
    follower.value = CellValue::Text("cache\r\nA1".into());
    follower.style.number_format = Some("0.000".into());
    let (bytes, xml) = exported_xml(&book);
    assert!(
        xml.contains(r#"<f t="shared" si="0" ref="B1:C2">"#),
        "{xml}"
    );
    let reopened = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    let follower = reopened.sheet(0).unwrap().get(at("C2")).unwrap();
    assert_eq!(
        follower.formula.as_deref(),
        Some("B2+$B3+D$3+$D$4+LEN(\"A1\")")
    );
    assert_eq!(follower.value, CellValue::Text("cache\r\nA1".into()));
    assert_eq!(follower.style.number_format.as_deref(), Some("0.000"));
}
#[test]
fn promoted_master_can_be_right_of_the_bounding_corner() {
    let mut book = read_workbook_bytes::<poirot::model::Workbook>(&package(&[r#"<row r="1"><c r="C1"><f t="shared" si="9" ref="B1:D3">C1+$F2</f></c></row><row r="2"><c r="D2"><f t="shared" si="9"/></c></row><row r="3"><c r="B3"><f t="shared" si="9"/></c></row>"#])).unwrap();
    mark_group(&mut book, &["C1", "D2", "B3"], "C1");
    book.sheet_mut(0).unwrap().remove(at("C1"));
    let (bytes, xml) = exported_xml(&book);
    assert!(
        xml.contains(r#"<f t="shared" si="0" ref="B2:D3">D2+$F3</f>"#),
        "{xml}"
    );
    let reopened = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    assert_eq!(
        reopened
            .sheet(0)
            .unwrap()
            .get(at("B3"))
            .unwrap()
            .formula
            .as_deref(),
        Some("B3+$F4")
    );
}
#[test]
fn group_origins_prevent_id_collisions_and_overlapping_output_bounds_fall_back() {
    use poirot::model::{Cell, SharedFormula};
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    // Two unrelated origins deliberately reuse the same public provenance ID.
    for (coordinate, source, origin) in [
        ("A1", "1", "A1"),
        ("A2", "1", "A1"),
        ("C1", "2", "C1"),
        ("C2", "2", "C1"),
    ] {
        let mut cell = Cell::with_formula(source, CellValue::Blank).unwrap();
        cell.shared_formula = Some(SharedFormula {
            group: SharedFormulaId(0),
            origin: at(origin),
        });
        sheet.set(at(coordinate), cell).unwrap();
    }
    let (bytes, xml) = exported_xml(&book);
    assert!(xml.contains(r#"si="0" ref="A1:A2""#));
    assert!(xml.contains(r#"si="1" ref="C1:C2""#));
    let reopened = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    assert_eq!(
        reopened
            .sheet(0)
            .unwrap()
            .get(at("A2"))
            .unwrap()
            .formula
            .as_deref(),
        Some("1")
    );
    assert_eq!(
        reopened
            .sheet(0)
            .unwrap()
            .get(at("C2"))
            .unwrap()
            .formula
            .as_deref(),
        Some("2")
    );
    let sheet = book.sheet_mut(0).unwrap();
    let cell = sheet.remove(at("A2")).unwrap();
    sheet.set(at("D2"), cell).unwrap();
    let (bytes, xml) = exported_xml(&book);
    assert!(!xml.contains(r#"t="shared""#), "{xml}");
    assert_eq!(
        read_workbook_bytes::<poirot::model::Workbook>(&bytes)
            .unwrap()
            .sheet(0)
            .unwrap()
            .get(at("D2"))
            .unwrap()
            .formula
            .as_deref(),
        Some("1")
    );
}

#[test]
fn unsupported_templates_and_stale_literal_members_never_omit_source() {
    for unsupported in [true, false] {
        let mut book = vertical_group();
        let sheet = book.sheet_mut(0).unwrap();
        if unsupported {
            for coordinate in ["B1", "B2", "B3"] {
                sheet.get_mut(at(coordinate)).unwrap().formula = Some("Table1[Column]".into());
            }
        } else {
            // Public mutation can leave a provenance tag on a newly literal cell.
            sheet.get_mut(at("B2")).unwrap().formula = None;
        }
        let (bytes, xml) = exported_xml(&book);
        assert!(!xml.contains(r#"t="shared""#));
        let reopened = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
        for coordinate in ["B1", "B2", "B3"] {
            assert_eq!(
                reopened
                    .sheet(0)
                    .unwrap()
                    .get(at(coordinate))
                    .unwrap()
                    .formula,
                book.sheet(0).unwrap().get(at(coordinate)).unwrap().formula
            );
        }
    }
}
#[test]
fn shared_bounds_must_not_overlap_independent_array_ownership() {
    let mut book = read_workbook_bytes::<poirot::model::Workbook>(&package(&[r#"<row r="1"><c r="A1"><f t="shared" si="0" ref="A1:C3">1</f><v>1</v></c><c r="B1"><f t="array" ref="B1:B2">{2;3}</f><v>2</v></c></row><row r="2"><c r="B2"><v>3</v></c></row><row r="3"><c r="C3"><f t="shared" si="0"/><v>1</v></c></row>"#])).unwrap();
    mark_group(&mut book, &["A1", "C3"], "A1");
    let (bytes, xml) = exported_xml(&book);
    assert!(!xml.contains(r#"t="shared""#));
    assert!(xml.contains(r#"<f t="array" ref="B1:B2">"#));
    let reopened = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    assert_eq!(
        reopened.sheet(0).unwrap().spill_owner(at("B2")),
        Some(at("B1"))
    );
    assert_eq!(
        reopened
            .sheet(0)
            .unwrap()
            .get(at("C3"))
            .unwrap()
            .formula
            .as_deref(),
        Some("1")
    );
}

#[test]
fn forward_shared_structured_source_survives_backend_translation() {
    let bytes = package(&[
        r#"<row><c r="A1"><f t="shared" si="0"/></c></row><row><c r="A2"><f t="shared" si="0" ref="A1:A2">Table1[Column]</f></c></row>"#,
    ]);
    let workbook = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    // A vertical shared copy leaves the structured column reference unchanged.
    assert_eq!(
        workbook
            .sheet(0)
            .unwrap()
            .get(at("A1"))
            .unwrap()
            .formula
            .as_deref(),
        Some("Table1[Column]")
    );
}

#[test]
fn modern_error_retry_preserves_following_implicit_coordinates_and_formulas() {
    let bytes = package(&[
        r#"<row><c t="e"><v>#SPILL!</v></c><c><f>1+2</f><v>3</v></c><c t="inlineStr"><is><r><t>rich</t></r><r><t> text</t></r></is></c></row><row><c t="e"><v>#CALC!</v></c><c><v>9</v></c></row>"#,
    ]);
    let workbook = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    let sheet = workbook.sheet(0).unwrap();
    assert_eq!(sheet.cells().len(), 5);
    assert_eq!(
        sheet.get(at("A1")).unwrap().value,
        CellValue::Error(poirot::model::CellError::Spill)
    );
    assert_eq!(sheet.get(at("B1")).unwrap().formula.as_deref(), Some("1+2"));
    assert_eq!(sheet.get(at("B1")).unwrap().value, CellValue::Number(3.));
    assert_eq!(
        sheet.get(at("C1")).unwrap().value,
        CellValue::Text("rich text".into())
    );
    assert_eq!(
        sheet.get(at("A2")).unwrap().value,
        CellValue::Error(poirot::model::CellError::Calc)
    );
    assert_eq!(sheet.get(at("B2")).unwrap().value, CellValue::Number(9.));
}
