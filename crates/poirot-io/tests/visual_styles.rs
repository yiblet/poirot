//! Independently handwritten OOXML fixtures, not exported snapshots or third-party data.
//! Semantics: Microsoft CellFormat/ForegroundColor/FontSize documentation and
//! MS-OE376 2.1.747 (apply flags govern future named-style propagation):
//! https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/59922f8b-0edc-4e93-a822-9f22254aec46
use poirot_io::{
    IoError, ReadOptions, WriteOptions, read_workbook_bytes, read_workbook_bytes_with_options,
    write_xlsx_to_buffer, write_xlsx_to_buffer_with_options,
};
use poirot_model::*;
use std::{
    io::{Cursor, Read, Write},
    sync::Arc,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const THEME: &str = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Handwritten &amp; local"><a:themeElements><a:clrScheme name="Colors"><a:accent1><a:srgbClr val="123456"/></a:accent1></a:clrScheme><a:fontScheme name="Fonts"/><a:fmtScheme name="Formats"/></a:themeElements></a:theme>"#;
const STYLES: &str = r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><numFmts count="1"><numFmt numFmtId="201" formatCode="0.000&amp;&quot; kg&quot;"/></numFmts><fonts count="999999999999999999999"><font><name val="Base"/><sz val="10"/></font><font><name val="Example &amp; Font"/><sz val="11.5"/><b/><i val="true"/><u val="double"/><strike/><color rgb="80123456"/><scheme val="minor"/><family val="2"/><charset val="1"/></font><font><color indexed="0"/></font></fonts><fills><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="solid"><fgColor theme="4" tint="0.25"/><bgColor indexed="1"/></patternFill></fill></fills><borders><border/><border><left style="thin"><color rgb="FFABCDEF"/></left><right style="double"><color auto="1"/></right><top style="dashed"><color indexed="0"/></top><bottom style="mediumDashDot"><color theme="5" tint="-0.5"/></bottom></border></borders><cellStyleXfs><xf fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="3"><xf fontId="0"/><xf fontId="1" fillId="1" borderId="1" numFmtId="201" xfId="0" applyFont="0"><alignment horizontal="centerContinuous" vertical="top" wrapText="1" shrinkToFit="true" textRotation="135" indent="2" readingOrder="2"/></xf><xf fontId="2"/></cellXfs><colors><indexedColors><rgbColor rgb="FF112233"/><rgbColor rgb="FF445566"/></indexedColors></colors></styleSheet>"#;
const SHEET: &str = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>3</v></c><c r="B1" s="1"><v>17</v></c><c r="C1" s="1"/><c r="D1" s="1"><f>B1+1</f><v>18</v></c><c r="E1" s="2"><v>2</v></c></row></sheetData></worksheet>"#;

/// Package handwritten parts at deliberately nonconventional OPC paths.
fn fixture(styles: &str, sheet: &str, theme: &str) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (path, xml) in [
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="documents/book.xml"/></Relationships>"#,
        ),
        (
            "documents/_rels/book.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="cells" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../data/cells.xml"/><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="../format/styles.xml"/><Relationship Id="theme" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../look/custom.xml"/></Relationships>"#,
        ),
        (
            "documents/book.xml",
            r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="cells"/></sheets></workbook>"#,
        ),
        ("data/cells.xml", sheet),
        ("format/styles.xml", styles),
        ("look/custom.xml", theme),
    ] {
        zip.start_file(path, SimpleFileOptions::default()).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
/// Read a generated XML part for independent structural assertions.
fn part(bytes: &[u8], name: &str) -> String {
    let mut z = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = String::new();
    z.by_name(name).unwrap().read_to_string(&mut out).unwrap();
    out
}
/// Borrow one fixture cell.
fn cell<'a>(book: &'a Workbook, address: &str) -> &'a Cell {
    book.sheet(0)
        .unwrap()
        .get(address.parse().unwrap())
        .unwrap()
}
#[test]
fn handwritten_components_context_default_blank_and_formula_are_preserved() {
    let book = read_workbook_bytes(&fixture(STYLES, SHEET, THEME)).unwrap();
    assert_eq!(book.style_context.theme(), Some(THEME.as_bytes()));
    assert_eq!(
        book.style_context.indexed_palette(),
        &[[255, 17, 34, 51], [255, 68, 85, 102]]
    );
    assert_eq!(book.style_context.default_style(), &cell(&book, "A1").style);
    assert!(book.sheet(0).unwrap().get("F1".parse().unwrap()).is_none());
    let s = &cell(&book, "B1").style;
    assert_eq!(s.number_format.as_deref(), Some("0.000&\" kg\""));
    let v = s.visual.as_deref().unwrap();
    let f = v.font.as_ref().unwrap();
    assert_eq!(f.name.as_deref(), Some("Example & Font"));
    assert_eq!(f.size.unwrap().get(), 11.5);
    assert!(f.bold && f.italic && f.strike);
    assert_eq!(f.underline, Some(Underline::Double));
    assert_eq!(
        f.color,
        Some(StyleColor {
            source: ColorSource::Argb([128, 18, 52, 86]),
            tint: None
        })
    );
    assert_eq!(f.scheme, Some(FontScheme::Minor));
    assert_eq!(f.family, Some(FontFamily::Swiss));
    assert_eq!(f.charset, Some(1));
    let fill = v.fill.as_ref().unwrap();
    assert_eq!(fill.pattern, PatternType::Solid);
    assert_eq!(
        fill.foreground,
        Some(StyleColor {
            source: ColorSource::Theme(ThemeColor::Accent1),
            tint: Some(Tint::new(0.25).unwrap())
        })
    );
    let b = v.borders.as_ref().unwrap();
    assert_eq!(b.left.as_ref().unwrap().line, Some(BorderLine::Thin));
    assert_eq!(
        b.right.as_ref().unwrap().color.unwrap().source,
        ColorSource::Automatic
    );
    assert_eq!(
        b.top.as_ref().unwrap().color.unwrap().source,
        ColorSource::Indexed(IndexedColor::new(0).unwrap())
    );
    assert_eq!(
        b.bottom.as_ref().unwrap().line,
        Some(BorderLine::MediumDashDot)
    );
    let a = v.alignment.as_ref().unwrap();
    assert_eq!(a.horizontal, Some(HorizontalAlignment::CenterContinuous));
    assert_eq!(a.vertical, Some(VerticalAlignment::Top));
    assert!(a.wrap_text && a.shrink_to_fit);
    assert_eq!(
        a.rotation,
        Some(TextRotation::Degrees(RotationDegrees::new(-45).unwrap()))
    );
    assert_eq!(a.indent, 2);
    assert_eq!(a.reading_order, Some(ReadingOrder::RightToLeft));
    assert_eq!(cell(&book, "C1").value, CellValue::Blank);
    assert_eq!(cell(&book, "D1").formula.as_deref(), Some("B1+1"));
    assert!(Arc::ptr_eq(
        s.visual.as_ref().unwrap(),
        cell(&book, "C1").style.visual.as_ref().unwrap()
    ));
    assert_eq!(
        cell(&book, "E1")
            .style
            .visual
            .as_ref()
            .unwrap()
            .font
            .as_ref()
            .unwrap()
            .color
            .unwrap()
            .source,
        ColorSource::Indexed(IndexedColor::new(0).unwrap())
    );
    let out = write_xlsx_to_buffer(&book).unwrap();
    let styles = part(&out, "xl/styles.xml");
    for expected in [
        r#"<name val="Example &amp; Font"/>"#,
        r#"<sz val="11.5"/>"#,
        r#"<u val="double"/>"#,
        r#"<color rgb="80123456"/>"#,
        r#"<fgColor theme="4" tint="0.25"/>"#,
        r#"<right style="double"><color auto="1"/></right>"#,
        r#"<bottom style="mediumDashDot"><color theme="5" tint="-0.5"/></bottom>"#,
        r#"horizontal="centerContinuous""#,
        r#"textRotation="135""#,
        r#"<rgbColor rgb="FF112233"/>"#,
        r#"formatCode="0.000&amp;&quot; kg&quot;""#,
    ] {
        assert!(styles.contains(expected), "{expected}\n{styles}");
    }
    let sheet = part(&out, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains(r#"<c r="A1"><v>3</v></c>"#));
    assert!(sheet.contains(r#"<c r="C1" s="1"></c>"#));
    assert!(sheet.contains(r#"<c r="D1" s="1"><f>B1+1</f><v>18</v></c>"#));
    let mut zip = ZipArchive::new(Cursor::new(&out)).unwrap();
    let mut theme_bytes = String::new();
    zip.by_name("xl/theme/poirot.xml")
        .unwrap()
        .read_to_string(&mut theme_bytes)
        .unwrap();
    assert_eq!(theme_bytes, THEME);
    assert!(part(&out, "xl/_rels/workbook.xml.rels").contains(r#"Target="/xl/theme/poirot.xml""#));
    assert!(
        part(&out, "[Content_Types].xml")
            .contains("application/vnd.openxmlformats-officedocument.theme+xml")
    );
    let reloaded = read_workbook_bytes(&out).unwrap();
    assert_eq!(cell(&reloaded, "B1").style, s.clone());
}
#[test]
fn dangling_references_and_malformed_supported_values_fail() {
    for (from, to) in [
        ("fontId=\"1\"", "fontId=\"3\""),
        ("fillId=\"1\"", "fillId=\"2\""),
        ("borderId=\"1\"", "borderId=\"2\""),
        ("xfId=\"0\"", "xfId=\"1\""),
        ("numFmtId=\"201\" xfId", "numFmtId=\"202\" xfId"),
        ("11.5", "NaN"),
        ("tint=\"0.25\"", "tint=\"2\""),
        ("theme=\"4\"", "theme=\"12\""),
        ("indexed=\"1\"", "indexed=\"66\""),
        ("textRotation=\"135\"", "textRotation=\"181\""),
        ("indent=\"2\"", "indent=\"65536\""),
        ("wrapText=\"1\"", "wrapText=\"maybe\""),
        ("rgb=\"80123456\"", "rgb=\"123456\""),
        ("style=\"thin\"", "style=\"unknown\""),
    ] {
        assert!(
            read_workbook_bytes(&fixture(&STYLES.replace(from, to), SHEET, THEME)).is_err(),
            "{from} -> {to}"
        );
    }
    assert!(
        read_workbook_bytes(&fixture(
            STYLES,
            &SHEET.replace("s=\"2\"", "s=\"3\""),
            THEME
        ))
        .is_err()
    );
}
#[test]
fn source_and_generated_xml_limits_refuse_styles_and_theme() {
    let input = fixture(STYLES, SHEET, THEME);
    assert!(matches!(
        read_workbook_bytes_with_options(
            &input,
            ReadOptions::default().max_zip_entry_bytes(STYLES.len() as u64 - 1)
        ),
        Err(IoError::Quota(_))
    ));
    let book = read_workbook_bytes(&input).unwrap();
    let output = write_xlsx_to_buffer(&book).unwrap();
    let size = part(&output, "xl/styles.xml").len() as u64;
    assert!(
        write_xlsx_to_buffer_with_options(
            &book,
            WriteOptions {
                max_zip_entry_bytes: size - 1,
                ..Default::default()
            }
        )
        .is_err()
    );
    let mut book = book;
    for theme in [
        "<theme xmlns=\"urn:wrong\"/>",
        "<!DOCTYPE a:theme SYSTEM \"https://example.invalid/dtd\"><a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"/>",
        "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"/><extra/>",
    ] {
        assert!(read_workbook_bytes(&fixture(STYLES, SHEET, theme)).is_err());
        book.style_context = StyleContext::new(Some(theme.as_bytes().to_vec()), vec![]).unwrap();
        assert!(write_xlsx_to_buffer(&book).is_err());
    }
}
#[test]
fn unsupported_styles_and_row_column_inheritance_warn_without_grid_expansion() {
    let styles = STYLES
        .replace(
            "<colors>",
            "<cellStyles><cellStyle name=\"Custom\" xfId=\"0\"/></cellStyles><extLst/><colors>",
        )
        .replace("<patternFill patternType=\"none\"/>", "<gradientFill/>")
        .replace("<border/>", "<border><diagonal/><start/></border>");
    let sheet = SHEET
        .replace(
            "<sheetData>",
            "<cols><col min=\"1\" max=\"16384\" style=\"1\"/></cols><sheetData>",
        )
        .replace("<row r=\"1\">", "<row r=\"1\" s=\"1\" customFormat=\"1\">");
    let book = read_workbook_bytes(&fixture(&styles, &sheet, THEME)).unwrap();
    assert_eq!(book.sheet(0).unwrap().cells().len(), 5);
    let warnings = &book.file_metadata.as_ref().unwrap().import_warnings;
    for text in [
        "gradientFill",
        "diagonal",
        "start",
        "cellStyles",
        "extLst",
        "row/column style inheritance",
    ] {
        assert!(warnings.iter().any(|w| w.detail.contains(text)), "{text}");
    }
}

#[test]
fn namespace_extensions_do_not_spoof_components_and_font_names_share_across_xfs() {
    let styles = STYLES
        .replace(
            "<sz val=\"11.5\"/>",
            "<x:sz xmlns:x=\"urn:extension\" val=\"999\"/><sz val=\"11.5\"/>",
        )
        .replace("<xf fontId=\"2\"/>", "<xf fontId=\"1\"/>");
    let theme = THEME.replace(
        "</a:theme>",
        "<a:extLst>A &amp; B &#38; &#x26;</a:extLst></a:theme>",
    );
    let book = read_workbook_bytes(&fixture(&styles, SHEET, &theme)).unwrap();
    let b = cell(&book, "B1").style.visual.as_ref().unwrap();
    let e = cell(&book, "E1").style.visual.as_ref().unwrap();
    assert!(!Arc::ptr_eq(b, e));
    assert!(Arc::ptr_eq(
        b.font.as_ref().unwrap().name.as_ref().unwrap(),
        e.font.as_ref().unwrap().name.as_ref().unwrap()
    ));
    assert_eq!(b.font.as_ref().unwrap().size.unwrap().get(), 11.5);
    assert!(
        book.file_metadata
            .as_ref()
            .unwrap()
            .import_warnings
            .iter()
            .any(|w| w.detail.contains("foreign namespace extensions"))
    );
    assert!(write_xlsx_to_buffer(&book).is_ok());
}
#[test]
fn missing_required_font_values_and_unsafe_xml_text_fail() {
    for (from, to) in [
        ("<sz val=\"11.5\"/>", "<sz/>"),
        ("<scheme val=\"minor\"/>", "<scheme/>"),
        ("<name val=\"Base\"/>", "<name/>"),
        ("<charset val=\"1\"/>", "<charset/>"),
        ("<family val=\"2\"/>", "<family/>"),
    ] {
        assert!(read_workbook_bytes(&fixture(&STYLES.replace(from, to), SHEET, THEME)).is_err());
    }
    for text in ["&undefined;", "&#0;"] {
        let theme = THEME.replace(
            "</a:theme>",
            &format!("<a:extLst>{text}</a:extLst></a:theme>"),
        );
        assert!(read_workbook_bytes(&fixture(STYLES, SHEET, &theme)).is_err());
    }
    let mut book = read_workbook_bytes(&fixture(STYLES, SHEET, THEME)).unwrap();
    book.sheet_mut(0)
        .unwrap()
        .get_mut("B1".parse().unwrap())
        .unwrap()
        .style
        .number_format = Some("bad\u{0}format".into());
    assert!(write_xlsx_to_buffer(&book).is_err());
}
