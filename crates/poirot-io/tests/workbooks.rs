use poirot_io::{
    IoError, ReadOptions, read_workbook, read_workbook_bytes, read_workbook_bytes_with_options,
    write_xlsx_to_buffer,
};
use poirot_model::{Cell, CellError, CellValue, DateSystem, Workbook};
use std::{
    io::{Cursor, Read},
    path::PathBuf,
};

fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn xml(bytes: &[u8], part: &str) -> String {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = String::new();
    zip.by_name(part).unwrap().read_to_string(&mut out).unwrap();
    out
}
fn address(s: &str) -> poirot_model::CellAddress {
    s.parse().unwrap()
}
fn cell<'a>(w: &'a Workbook, s: &str) -> &'a Cell {
    w.sheet(0).unwrap().get(address(s)).unwrap()
}

#[test]
fn real_poi_xls_and_xlsx_preserve_sparse_values_and_empty_sheets() {
    for file in ["SimpleMultiCell.xlsx", "SimpleMultiCell.xls"] {
        let w = read_workbook(
            repository()
                .join("crates/poirot-io/tests/fixtures")
                .join(format!("poi-{file}")),
        )
        .unwrap();
        assert_eq!(w.sheets().len(), 3, "{file}");
        assert_eq!(cell(&w, "A1").value, CellValue::Number(1.));
        assert_eq!(cell(&w, "E5").value, CellValue::Number(15.));
        assert!(w.sheet(0).unwrap().get(address("E1")).is_none());
        assert!(w.sheet(1).unwrap().cells().is_empty());
    }
}

#[test]
fn original_stale_cache_survives_import_and_formula_is_preserved() {
    let w = read_workbook(repository().join("crates/poirot-io/tests/fixtures/stale-cache.xlsx"))
        .unwrap();
    assert_eq!(cell(&w, "A3").value, CellValue::Number(777.));
    assert_eq!(cell(&w, "A3").formula.as_deref(), Some("SUM(A1:A2)"));
    assert_eq!(cell(&w, "A6").value, CellValue::Text("STALE".into()));
    assert_eq!(cell(&w, "A7").value, CellValue::Boolean(false));
    assert_eq!(cell(&w, "A9").value, CellValue::Blank);
}

#[test]
fn typed_caches_literals_names_dates_and_raw_format_roundtrip() {
    let mut w = Workbook::new();
    w.date_system = DateSystem::Excel1904;
    let sheet = w.add_sheet("Data & Results").unwrap();
    let values = [
        CellValue::Number(-0.0),
        CellValue::Text("123".into()),
        CellValue::Text("TRUE".into()),
        CellValue::Text("#DIV/0!".into()),
        CellValue::Text("a<&\"b".into()),
        CellValue::Text(String::new()),
        CellValue::Boolean(true),
        CellValue::Error(CellError::Div0),
        CellValue::Blank,
    ];
    for (i, value) in values.iter().enumerate() {
        sheet
            .set_formula_with_cached_value(
                poirot_model::CellAddress::new(i as u32, 0).unwrap(),
                "IF(1<2,\"a&b\",\"c\")",
                value.clone(),
            )
            .unwrap();
    }
    sheet
        .set_value(address("B1"), CellValue::Error(CellError::NA))
        .unwrap();
    sheet
        .set_value(address("B2"), "  whitespace & < 😀  ")
        .unwrap();
    sheet
        .set_value(address("B3"), CellValue::Text(String::new()))
        .unwrap();
    sheet.set_value(address("C10"), 123.4).unwrap();
    sheet.get_mut(address("C10")).unwrap().style.number_format =
        Some("[Red]0.00;\"minus & <\"0.00".into());
    w.add_sheet("Empty").unwrap();
    w.define_name("Rate", "0.1", None).unwrap();
    w.define_name("Rate", "0.2", Some(0)).unwrap();
    let bytes = write_xlsx_to_buffer(&w).unwrap();
    let raw = xml(&bytes, "xl/worksheets/sheet1.xml");
    // Independent byte-level assertions catch numeric-string/boolean/error guessing.
    assert!(raw.contains("r=\"A2\" t=\"str\""), "{raw}");
    assert!(raw.contains("r=\"A3\" t=\"str\""));
    assert!(raw.contains("r=\"A4\" t=\"str\""));
    assert!(raw.contains("r=\"A7\" t=\"b\""));
    assert!(raw.contains("r=\"A8\" t=\"e\""));
    assert!(raw.contains("r=\"B1\" t=\"e\""));
    assert!(raw.contains("<v>-0</v>"));
    assert!(raw.contains("<f>IF(1&lt;2,&quot;a&amp;b&quot;,&quot;c&quot;)</f>"));
    let book_xml = xml(&bytes, "xl/workbook.xml");
    assert!(book_xml.contains("date1904=\"1\""));
    assert!(book_xml.contains("localSheetId=\"0\""));
    let imported = read_workbook_bytes(&bytes).unwrap();
    let actual = imported;
    assert_eq!(actual.date_system, DateSystem::Excel1904);
    assert_eq!(actual.sheets().len(), 2);
    assert!(actual.sheet(1).unwrap().cells().is_empty());
    for (i, value) in values.iter().enumerate() {
        assert_eq!(
            actual
                .sheet(0)
                .unwrap()
                .get(poirot_model::CellAddress::new(i as u32, 0).unwrap())
                .unwrap()
                .value,
            *value,
            "row {i}"
        );
    }
    assert_eq!(cell(&actual, "B1").value, CellValue::Error(CellError::NA));
    assert_eq!(
        cell(&actual, "B2").value,
        CellValue::Text("  whitespace & < 😀  ".into())
    );
    assert_eq!(cell(&actual, "B3").value, CellValue::Text(String::new()));
    assert_eq!(
        cell(&actual, "C10").style.number_format,
        cell(&w, "C10").style.number_format
    );
    assert_eq!(actual.defined_name("Rate", 0).unwrap().formula, "0.2");
    assert_eq!(actual.defined_name("Rate", 1).unwrap().formula, "0.1");
    assert!(
        !actual
            .file_metadata
            .as_ref()
            .unwrap()
            .import_warnings
            .is_empty()
    );
}

#[test]
fn real_lo_export_retains_native_export_error_channel_without_reinterpreting_it() {
    let path = repository().join("crates/poirot-io/tests/fixtures/libreoffice-recalculated.xlsx");
    let w = read_workbook(path).unwrap();
    assert_eq!(cell(&w, "A3").value, CellValue::Number(5.));
    assert_eq!(cell(&w, "A6").value, CellValue::Text("yes".into()));
    assert_eq!(cell(&w, "A8").value, CellValue::Text(String::new()));
    assert_eq!(cell(&w, "A12").value, CellValue::Error(CellError::Value));
    assert_eq!(cell(&w, "A17").value, CellValue::Number(4.));
}

#[test]
fn configured_small_budgets_and_bad_files_fail_explicitly() {
    let mut w = Workbook::new();
    w.add_sheet("S")
        .unwrap()
        .set_value(address("A1"), 1.0)
        .unwrap();
    let bytes = write_xlsx_to_buffer(&w).unwrap();
    for limits in [
        ReadOptions {
            max_input_bytes: 1,
            ..ReadOptions::default()
        },
        ReadOptions {
            max_zip_entry_bytes: 1,
            ..ReadOptions::default()
        },
        ReadOptions {
            max_zip_total_bytes: 1,
            ..ReadOptions::default()
        },
        ReadOptions {
            max_zip_entries: 1,
            ..ReadOptions::default()
        },
    ] {
        assert!(matches!(
            read_workbook_bytes_with_options(&bytes, limits),
            Err(IoError::Limit(_))
        ));
    }
    assert!(read_workbook_bytes(b"not a ZIP").is_err());
    assert!(write_xlsx_to_buffer(&Workbook::new()).is_err());
    w.sheet_mut(0)
        .unwrap()
        .set_value(address("A1"), f64::NAN)
        .unwrap();
    assert!(write_xlsx_to_buffer(&w).is_err());
}

#[test]
fn identifier_spelling_scope_and_permissive_names_survive_raw_xml() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Ä Data").unwrap();
    workbook.add_sheet("Other").unwrap();
    // Imported names intentionally bypass authoring grammar, and duplicate spelling
    // across scopes is valid. The exporter must preserve each raw identity.
    for (name, formula, scope) in [
        ("ÉRate", "1", None),
        ("érate", "2", Some(0)),
        ("imported odd name", "3", Some(1)),
    ] {
        workbook.named_ranges.push(poirot_model::NamedRange {
            name: name.into(),
            formula: formula.into(),
            sheet_scope: scope,
        });
    }
    let bytes = write_xlsx_to_buffer(&workbook).unwrap();
    let raw = xml(&bytes, "xl/workbook.xml");
    assert!(raw.contains("name=\"Ä Data\""), "{raw}");
    assert!(raw.contains("name=\"ÉRate\""), "{raw}");
    assert!(raw.contains("name=\"érate\" localSheetId=\"0\""), "{raw}");
    assert!(
        raw.contains("name=\"imported odd name\" localSheetId=\"1\""),
        "{raw}"
    );
    let actual = read_workbook_bytes(&bytes).unwrap();
    assert_eq!(actual.sheets()[0].name(), "Ä Data");
    assert_eq!(actual.named_ranges, workbook.named_ranges);
    assert_eq!(actual.try_sheet_index_by_name("ä data"), Ok(Some(0)));
    assert_eq!(
        actual
            .try_defined_name("ÉRATE", 0)
            .unwrap()
            .unwrap()
            .formula,
        "2"
    );
}

#[test]
fn file_and_buffer_routes_retain_the_same_workbook_and_warnings() {
    let directory = std::env::temp_dir().join(format!("poirot-read-routes-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (extension, format) in [
        ("xlsx", poirot_model::FileFormat::Xlsx),
        ("xls", poirot_model::FileFormat::Xls),
    ] {
        let fixture = repository()
            .join("crates/poirot-io/tests/fixtures")
            .join(format!("poi-SimpleMultiCell.{extension}"));
        let bytes = std::fs::read(fixture).unwrap();
        let path = directory.join(format!("{extension}.unrelated-extension"));
        std::fs::write(&path, &bytes).unwrap();
        let expected = read_workbook_bytes(&bytes).unwrap();
        let metadata = expected.file_metadata.as_ref().unwrap();
        assert_eq!(metadata.format, format);
        assert!(
            metadata
                .import_warnings
                .iter()
                .any(|warning| warning.kind == poirot_model::ImportDiagnosticKind::OmittedFeatures)
        );
        for actual in [
            read_workbook(&path).unwrap(),
            poirot_io::read_workbook_with_options(&path, ReadOptions::default()).unwrap(),
            read_workbook_bytes_with_options(&bytes, ReadOptions::default()).unwrap(),
        ] {
            assert_eq!(actual, expected, "{extension}");
        }
        let options = ReadOptions {
            max_input_bytes: 1,
            ..ReadOptions::default()
        };
        assert!(matches!(
            poirot_io::read_workbook_with_options(&path, options),
            Err(IoError::Limit(_))
        ));
        assert!(matches!(
            read_workbook_bytes_with_options(&bytes, options),
            Err(IoError::Limit(_))
        ));
        std::fs::write(&path, b"unknown file signature").unwrap();
        assert!(matches!(read_workbook(&path), Err(IoError::Unsupported(_))));
        assert!(matches!(
            read_workbook_bytes(b"unknown file signature"),
            Err(IoError::Unsupported(_))
        ));
    }
    std::fs::remove_dir_all(directory).unwrap();
}
