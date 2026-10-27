use poirot::model::{Cell, CellAddress, CellValue, ExcelText, Workbook};
use poirot_io::{
    IoError, WriteOptions, plan_xlsx_export, read_workbook_bytes, write_xlsx_to_buffer_with_options,
};
use std::io::{Cursor, Read};

#[test]
fn xlsx_roundtrip_retains_units_formula_caches_and_escape_collisions() {
    let values = [
        vec![0xd83d, 0xde00],
        vec![0xd83d],
        vec![0xde00],
        "_xD83D_".encode_utf16().collect(),
        vec![0, 0xffff, 65],
    ];
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("Data").unwrap();
    for (index, units) in values.iter().enumerate() {
        let value = CellValue::TextUtf16(ExcelText::from_units(units.clone()));
        sheet
            .set(
                CellAddress::new(0, index as u32).unwrap(),
                Cell::new(value.clone()),
            )
            .unwrap();
        sheet
            .set_formula_with_cached_value(CellAddress::new(1, index as u32).unwrap(), "1", value)
            .unwrap();
    }
    let bytes = write_xlsx_to_buffer_with_options(&workbook, WriteOptions::default()).unwrap();
    let before = bytes.clone();
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("xl/worksheets/sheet1.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("_xD83D_"));
    assert!(xml.contains("_xDE00_"));
    assert!(xml.contains("_x005F_xD83D_"));
    let report = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    for row in 0..2 {
        for (index, units) in values.iter().enumerate() {
            let cell = report
                .sheet(0)
                .unwrap()
                .get(CellAddress::new(row, index as u32).unwrap())
                .unwrap();
            assert_eq!(
                cell.unwrap()
                    .value
                    .text_ref()
                    .unwrap()
                    .units()
                    .collect::<Vec<_>>(),
                *units
            );
            assert_eq!(
                cell.unwrap().formula.as_deref(),
                if row == 1 { Some("1") } else { None }
            );
        }
    }
    assert_eq!(before, bytes);
}

#[test]
fn utf16_storage_admission_and_escaped_output_limits_are_ordered() {
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Data")
        .unwrap()
        .set(
            "A1",
            Cell::new(CellValue::TextUtf16(ExcelText::from_units(vec![
                0xd83d;
                4000
            ]))),
        )
        .unwrap();
    let plan = plan_xlsx_export(&workbook, WriteOptions::default()).unwrap();
    assert_eq!(plan.source_bytes(), 4 + 24 + 8000);
    assert!(matches!(
        plan_xlsx_export(
            &workbook,
            WriteOptions {
                max_source_bytes: 8027,
                ..WriteOptions::default()
            }
        ),
        Err(IoError::Limit(_))
    ));
    let error = write_xlsx_to_buffer_with_options(
        &workbook,
        WriteOptions {
            max_zip_entry_bytes: 10_000,
            ..WriteOptions::default()
        },
    )
    .unwrap_err();
    assert!(
        matches!(error, IoError::Limit(poirot_io::ExportLimitError::PackageBytes {entry_bytes,entry_limit:10_000,total_bytes,total_limit}) if entry_bytes > 10_000 && total_bytes >= entry_bytes && total_limit == WriteOptions::default().max_zip_total_bytes)
    );
    assert!(matches!(
        write_xlsx_to_buffer_with_options(
            &workbook,
            WriteOptions {
                max_output_bytes: 1,
                ..WriteOptions::default()
            }
        ),
        Err(IoError::Limit(_))
    ));
}
