use poirot::model::{Cell, CellValue, Workbook};
use poirot_io::{IoError, WriteOptions, plan_xlsx_export, write_xlsx_to_buffer_with_options};
use std::io::Cursor;

#[test]
fn plan_is_pure_borrowed_and_wrapper_content_matches_except_creation_clock() {
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Data")
        .unwrap()
        .set("A1", Cell::new(CellValue::Text("payload".into())))
        .unwrap();
    let before = format!("{workbook:?}");
    let plan = plan_xlsx_export(&workbook, WriteOptions::default()).unwrap();
    assert!(std::ptr::eq(plan.source(), &workbook));
    assert_eq!(plan.cell_count(), 1);
    // Sheet name, admitted row metadata, and text payload.
    assert_eq!(plan.source_bytes(), 4 + 24 + 7);
    assert_eq!(format!("{:?}", plan.source()), before);
    let planned = plan.execute().unwrap();
    let wrapped = write_xlsx_to_buffer_with_options(&workbook, plan.options()).unwrap();
    let mut left = zip::ZipArchive::new(Cursor::new(planned)).unwrap();
    let mut right = zip::ZipArchive::new(Cursor::new(wrapped)).unwrap();
    assert_eq!(left.len(), right.len());
    for i in 0..left.len() {
        use std::io::Read;
        let mut part = left.by_index(i).unwrap();
        let name = part.name().unwrap().into_owned();
        let mut expected = Vec::new();
        let mut actual = Vec::new();
        right
            .by_name(&name)
            .unwrap()
            .read_to_end(&mut expected)
            .unwrap();
        part.read_to_end(&mut actual).unwrap();
        // Backend creation clocks are removed: every part is deterministic.
        assert_eq!(actual, expected, "{}", name);
    }
    assert_eq!(format!("{workbook:?}"), before);
    assert!(matches!(
        plan_xlsx_export(
            &workbook,
            WriteOptions {
                max_source_bytes: 10,
                ..WriteOptions::default()
            }
        ),
        Err(IoError::Limit(_))
    ));
}

#[test]
fn typed_cache_patch_is_counted_before_allocation_and_output() {
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Data")
        .unwrap()
        .set_formula_with_cached_value("A1", "1", CellValue::Text("&".repeat(4000)))
        .unwrap();
    let options = WriteOptions {
        max_zip_entry_bytes: 30_000,
        ..WriteOptions::default()
    };
    let bytes = write_xlsx_to_buffer_with_options(&workbook, options).unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    assert!(zip.by_name("xl/worksheets/sheet1.xml").unwrap().size() > 20_000);
    let failure = write_xlsx_to_buffer_with_options(
        &workbook,
        WriteOptions {
            max_zip_entry_bytes: 10_000,
            ..options
        },
    )
    .unwrap_err();
    assert!(
        matches!(failure, IoError::Limit(poirot_io::ExportLimitError::PackageBytes {entry_bytes,entry_limit:10_000,total_bytes,total_limit}) if entry_bytes > 10_000 && total_bytes >= entry_bytes && total_limit == options.max_zip_total_bytes)
    );
    assert!(matches!(
        write_xlsx_to_buffer_with_options(
            &workbook,
            WriteOptions {
                max_output_bytes: 1,
                ..options
            }
        ),
        Err(IoError::Limit(_))
    ));
}

/// Exercises the 32 MiB per-entry limit using typed formula caches.
/// Run explicitly: `cargo test --test integration adequate_writer_budget -- --ignored`.
#[test]
#[ignore = "large workbook resource profile; run explicitly"]
fn adequate_writer_budget_crosses_former_hidden_entry_threshold() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("Large").unwrap();
    for row in 0..1200 {
        sheet
            .set_formula_with_cached_value(
                poirot::model::CellAddress::new(row, 0).unwrap(),
                "1",
                CellValue::Text("x".repeat(30_000)),
            )
            .unwrap();
    }
    let options = WriteOptions {
        max_zip_entry_bytes: 64 * 1024 * 1024,
        max_zip_total_bytes: 128 * 1024 * 1024,
        ..WriteOptions::default()
    };
    let bytes = write_xlsx_to_buffer_with_options(&workbook, options).unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    assert!(zip.by_name("xl/worksheets/sheet1.xml").unwrap().size() > 32 * 1024 * 1024);
    assert!(matches!(
        write_xlsx_to_buffer_with_options(&workbook, WriteOptions::default()),
        Err(IoError::Limit(_))
    ));
}

#[test]
fn repeated_literals_and_formula_caches_use_budgeted_inline_text() {
    let text = "x".repeat(1000);
    for formula in [false, true] {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("Data").unwrap();
        for row in 0..12 {
            let address = poirot::model::CellAddress::new(row, 0).unwrap();
            if formula {
                sheet
                    .set_formula_with_cached_value(address, "1", CellValue::Text(text.clone()))
                    .unwrap();
            } else {
                sheet
                    .set(address, Cell::new(CellValue::Text(text.clone())))
                    .unwrap();
            }
        }
        let options = WriteOptions {
            max_zip_entry_bytes: 20_000,
            ..WriteOptions::default()
        };
        // poirot ooxml::write_cells uses inlineStr for literals and str for caches:
        // both repeat the full text in each final worksheet cell.
        let bytes = write_xlsx_to_buffer_with_options(&workbook, options).unwrap();
        let mut final_zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert!(
            final_zip
                .by_name("xl/worksheets/sheet1.xml")
                .unwrap()
                .size()
                > 12_000
        );
        let small = WriteOptions {
            max_zip_entry_bytes: 10_000,
            ..options
        };
        assert!(matches!(
            plan_xlsx_export(&workbook, small),
            Err(IoError::Limit(_))
        ));
        assert!(matches!(
            plan_xlsx_export(
                &workbook,
                WriteOptions {
                    max_source_bytes: 11_999,
                    ..options
                }
            ),
            Err(IoError::Limit(_))
        ));
    }
}

/// Build a workbook whose cells share a few styles, like realistic models.
fn styled_grid(rows: u32, cols: u32) -> Workbook {
    use poirot::model::{CellAddress, CellStyle, FontStyle, VisualStyle};
    use std::sync::Arc;
    let bold = Arc::new(VisualStyle {
        font: Some(FontStyle {
            bold: true,
            ..FontStyle::default()
        }),
        ..VisualStyle::default()
    });
    let styles = [
        CellStyle {
            number_format: Some("0.00".into()),
            visual: None,
        },
        CellStyle {
            number_format: None,
            visual: Some(bold.clone()),
        },
        CellStyle {
            number_format: Some("#,##0".into()),
            visual: Some(bold),
        },
    ];
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("Model").unwrap();
    for row in 0..rows {
        for col in 0..cols {
            let address = CellAddress::new(row, col).unwrap();
            let mut cell = match col % 3 {
                0 => Cell::new(CellValue::Number(f64::from(row * cols + col))),
                1 => Cell::new(CellValue::Text(format!("r{row}"))),
                _ => {
                    let mut cell = Cell::new(CellValue::Number(f64::from(row + 1)));
                    cell.formula = Some(format!("ROW({address})"));
                    cell
                }
            };
            cell.style = styles[((row + col) % 3) as usize].clone();
            sheet.set(address, cell).unwrap();
        }
    }
    workbook
}

/// Verify styles, literals and formula caches survive an XLSX round trip.
fn assert_styled_round_trip(rows: u32, cols: u32) {
    let workbook = styled_grid(rows, cols);
    let bytes = write_xlsx_to_buffer_with_options(&workbook, WriteOptions::default()).unwrap();
    let restored = poirot_io::read_workbook_bytes::<Workbook>(&bytes).unwrap();
    let (source, restored) = (workbook.sheet(0).unwrap(), restored.sheet(0).unwrap());
    assert_eq!(restored.name(), "Model");
    assert_eq!(restored.cells().len(), (rows * cols) as usize);
    assert!(source.cells() == restored.cells());
}

#[test]
fn styled_workbook_exports_and_round_trips() {
    assert_styled_round_trip(4, 3);
}

#[test]
fn write_options_still_refuse_oversize_output_before_returning_bytes() {
    // Keep the final package larger than the transient writer package so the
    // exact output-byte limit below also admits the intermediate package.
    let workbook = styled_grid(20, 30);
    let unrestricted = write_xlsx_to_buffer_with_options(&workbook, WriteOptions::default())
        .unwrap()
        .len() as u64;
    for options in [
        WriteOptions {
            max_output_bytes: unrestricted - 1,
            ..WriteOptions::default()
        },
        WriteOptions {
            max_zip_entry_bytes: 256,
            ..WriteOptions::default()
        },
        WriteOptions {
            max_zip_total_bytes: 512,
            ..WriteOptions::default()
        },
        WriteOptions {
            max_zip_entries: 4,
            ..WriteOptions::default()
        },
    ] {
        assert!(
            matches!(
                write_xlsx_to_buffer_with_options(&workbook, options),
                Err(IoError::Limit(_))
            ),
            "{options:?}"
        );
    }
    let exact = WriteOptions {
        max_output_bytes: unrestricted,
        ..WriteOptions::default()
    };
    assert_eq!(
        write_xlsx_to_buffer_with_options(&workbook, exact)
            .unwrap()
            .len() as u64,
        unrestricted
    );
}

#[test]
fn exports_omit_backend_theme_and_creation_clock() {
    use std::io::Read;
    let workbook = styled_grid(2, 3);
    let bytes = write_xlsx_to_buffer_with_options(&workbook, WriteOptions::default()).unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let names: Vec<String> = zip
        .file_names()
        .map(|name| name.unwrap().into_owned())
        .collect();
    assert!(
        !names.iter().any(|name| name.starts_with("xl/theme/")),
        "{names:?}"
    );
    let mut part = |name: &str| {
        let mut text = String::new();
        zip.by_name(name)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        text
    };
    assert!(!part("[Content_Types].xml").contains("theme"));
    assert!(!part("xl/_rels/workbook.xml.rels").contains("theme"));
    let core = part("docProps/core.xml");
    assert!(
        !core.contains("created") && !core.contains("modified"),
        "{core}"
    );
    let restored = poirot_io::read_workbook_bytes::<Workbook>(&bytes).unwrap();
    assert_eq!(restored.style_context.theme(), None);
}
