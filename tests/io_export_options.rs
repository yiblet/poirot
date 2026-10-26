use poirot::model::{Cell, CellValue, Workbook};
use poirot_io::{IoError, WriteOptions, plan_xlsx_export, write_xlsx_to_buffer_with_options};
use std::io::Cursor;
#[path = "io_support/export_clock.rs"]
mod export_clock;

#[test]
fn plan_is_pure_borrowed_and_wrapper_content_matches_except_creation_clock() {
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Data")
        .unwrap()
        .set(
            "A1".parse().unwrap(),
            Cell::new(CellValue::Text("payload".into())),
        )
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
        // rxls fresh authoring has no wall-clock defaults: every part is deterministic.
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
        .set_formula_with_cached_value(
            "A1".parse().unwrap(),
            "1",
            CellValue::Text("&".repeat(4000)),
        )
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
        matches!(failure, IoError::Limit(ref message) if message.contains("before allocation"))
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
/// Run explicitly: `cargo test -p poirot-io --test export_options -- --ignored`.
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
