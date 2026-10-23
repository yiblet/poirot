//! Independent storage tests from Microsoft's MS-XLSX §2.2.3 function table.
//! https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/5d1b6d44-6fc1-4ecd-8fef-0b27406cc2bf
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
use poirot_model::Workbook;
use std::io::{Cursor, Read};
#[test]
fn writer_encodes_modern_functions_and_reader_retains_storage_spelling() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    let examples = [
        (
            "A1",
            "FILTER({2;5},{TRUE;FALSE})",
            "_xlfn._xlws.FILTER({2;5},{TRUE;FALSE})",
        ),
        ("D1", "SORT({5;2})", "_xlfn._xlws.SORT({5;2})"),
        ("G1", "TAKE({2;5},1)", "_xlfn.TAKE({2;5},1)"),
        (
            "J1",
            "TEXTBEFORE(\"a:b\",\":\")",
            "_xlfn.TEXTBEFORE(\"a:b\",\":\")",
        ),
        ("M1", "_xlfn._xlws.SORT({8;3})", "_xlfn._xlws.SORT({8;3})"),
        ("P1", "IFNA(#N/A,7)", "_xlfn.IFNA(#N/A,7)"),
        ("S1", "NORM.DIST(0,0,1,TRUE)", "_xlfn.NORM.DIST(0,0,1,TRUE)"),
        (
            "V1",
            "SUM(_xlfn.TAKE({2;5},1),TAKE({3;8},1))",
            "SUM(_xlfn.TAKE({2;5},1),_xlfn.TAKE({3;8},1))",
        ),
        ("Y1", "LEN(\"FILTER(1)\")", "LEN(\"FILTER(1)\")"),
        ("AB1", "_xlfn._xlws.UNKNOWN(1)", "_xlfn._xlws.UNKNOWN(1)"),
    ];
    for (cell, formula, _) in examples {
        sheet.set_formula(cell.parse().unwrap(), formula).unwrap();
    }
    let bytes = write_xlsx_to_buffer(&book).unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("xl/worksheets/sheet1.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    let reopened = read_workbook_bytes(&bytes).unwrap();
    for (cell, _, stored) in examples {
        assert!(
            xml.contains(&stored.replace('"', "&quot;")),
            "{stored}: {xml}"
        );
        assert_eq!(
            reopened
                .sheet(0)
                .unwrap()
                .get(cell.parse().unwrap())
                .unwrap()
                .formula
                .as_deref(),
            Some(stored)
        );
    }
}
#[test]
fn regex_names_are_encoded_without_rewriting_pattern_literals() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    let examples = [
        (
            "A1",
            r#"REGEXTEST("REGEXTEST(x)","REGEXTEST")"#,
            r#"_xlfn.REGEXTEST("REGEXTEST(x)","REGEXTEST")"#,
        ),
        (
            "D1",
            r#"REGEXEXTRACT("a1 b2","\d+",1)"#,
            r#"_xlfn.REGEXEXTRACT("a1 b2","\d+",1)"#,
        ),
        (
            "G1",
            r#"REGEXREPLACE("a1","([a-z])(\d)","$2:$1")"#,
            r#"_xlfn.REGEXREPLACE("a1","([a-z])(\d)","$2:$1")"#,
        ),
    ];
    for (cell, source, _) in examples {
        sheet.set_formula(cell.parse().unwrap(), source).unwrap();
    }
    let bytes = write_xlsx_to_buffer(&book).unwrap();
    let reopened = read_workbook_bytes(&bytes).unwrap();
    for (cell, _, stored) in examples {
        assert_eq!(
            reopened
                .sheet(0)
                .unwrap()
                .get(cell.parse().unwrap())
                .unwrap()
                .formula
                .as_deref(),
            Some(stored)
        );
    }
}

#[test]
fn random_and_trim_storage_preserve_literals_and_arguments() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (at, source) in [
        ("A1", "RANDARRAY(2,3,-5,7,TRUE)"),
        ("G1", r#"TRIMRANGE(A1:D5,1,2)&"TRIMRANGE(1)""#),
    ] {
        sheet.set_formula(at.parse().unwrap(), source).unwrap();
    }
    let reopened = read_workbook_bytes(&write_xlsx_to_buffer(&book).unwrap()).unwrap();
    for (at, expected) in [
        ("A1", "_xlfn.RANDARRAY(2,3,-5,7,TRUE)"),
        ("G1", r#"_xlfn.TRIMRANGE(A1:D5,1,2)&"TRIMRANGE(1)""#),
    ] {
        assert_eq!(
            reopened
                .sheet(0)
                .unwrap()
                .get(at.parse().unwrap())
                .unwrap()
                .formula
                .as_deref(),
            Some(expected)
        );
    }
}
#[test]
fn grouping_storage_keeps_aggregation_identifiers_and_string_literals() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    let source = r#"GROUPBY({1;1},{2;8},HSTACK(SUM,PERCENTOF),0,0)&"GROUPBY(1)""#;
    sheet.set_formula("A1".parse().unwrap(), source).unwrap();
    let bytes = write_xlsx_to_buffer(&book).unwrap();
    let reopened = read_workbook_bytes(&bytes).unwrap();
    assert_eq!(
        reopened
            .sheet(0)
            .unwrap()
            .get("A1".parse().unwrap())
            .unwrap()
            .formula
            .as_deref(),
        Some(r#"_xlfn.GROUPBY({1;1},{2;8},_xlfn.HSTACK(SUM,PERCENTOF),0,0)&"GROUPBY(1)""#)
    );
}

#[test]
fn scalar_utility_storage_uses_normative_names_and_preserves_literals() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    let examples = [
        ("A1", "MUNIT(2)", "_xlfn.MUNIT(2)"),
        (
            "D1",
            "NETWORKDAYS.INTL(45292,45298,1)",
            "NETWORKDAYS.INTL(45292,45298,1)",
        ),
        (
            "G1",
            "ACOT(1)+ISO.CEILING(1.2)",
            "_xlfn.ACOT(1)+ISO.CEILING(1.2)",
        ),
        (
            "J1",
            r#"EUROCONVERT(1,"DEM","EUR")&"MUNIT(2)""#,
            r#"EUROCONVERT(1,"DEM","EUR")&"MUNIT(2)""#,
        ),
        (
            "M1",
            "MULTINOMIAL(2,3)+XOR(TRUE,FALSE)",
            "MULTINOMIAL(2,3)+_xlfn.XOR(TRUE,FALSE)",
        ),
    ];
    for (cell, source, _) in examples {
        sheet.set_formula(cell.parse().unwrap(), source).unwrap();
    }
    let reopened = read_workbook_bytes(&write_xlsx_to_buffer(&book).unwrap()).unwrap();
    for (cell, _, stored) in examples {
        assert_eq!(
            reopened
                .sheet(0)
                .unwrap()
                .get(cell.parse().unwrap())
                .unwrap()
                .formula
                .as_deref(),
            Some(stored)
        );
    }
}
