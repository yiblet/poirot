use poirot::model::{CellAddress, CellValue, MAX_ROWS, Workbook};
use poirot_io as io;

fn source(last: u32) -> Workbook {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("Rows").unwrap();
    sheet.create_row(0).unwrap();
    sheet.create_row(last).unwrap();
    let address = CellAddress::new(12, 3).unwrap();
    sheet.set_value(address, 7.0).unwrap();
    sheet
        .set_value(CellAddress::new(5, 0).unwrap(), 2.0)
        .unwrap();
    sheet.remove(CellAddress::new(5, 0).unwrap()).unwrap();
    sheet.create_row(6).unwrap();
    sheet.remove_row(6).unwrap();
    workbook
}
fn assert_rows(workbook: &Workbook, last: u32) {
    let sheet = workbook.sheet(0).unwrap();
    assert_eq!(
        sheet.row_indices().collect::<Vec<_>>(),
        vec![0, 5, 12, last]
    );
    assert!(sheet.row(0).unwrap().cells().next().is_none());
    assert!(sheet.row(5).unwrap().get(0).is_none());
    assert!(sheet.row(6).is_none());
    assert_eq!(
        sheet.row(12).unwrap().get(3).unwrap().value,
        CellValue::Number(7.0)
    );
    assert_eq!(sheet.cells().len(), 1);
}
#[test]
fn actual_xlsx_roundtrip_retains_empty_sparse_rows() {
    let workbook = source(MAX_ROWS - 1);
    let bytes = io::write_xlsx_to_buffer(&workbook).unwrap();
    assert_rows(
        &io::read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap(),
        MAX_ROWS - 1,
    );
    let mut empty = Workbook::new();
    empty.add_sheet("Empty").unwrap().create_row(44).unwrap();
    let bytes = io::write_xlsx_to_buffer(&empty).unwrap();
    assert!(
        io::read_workbook_bytes::<poirot::model::Workbook>(&bytes)
            .unwrap()
            .sheet(0)
            .unwrap()
            .row(44)
            .is_some()
    );
}
#[test]
fn empty_rows_are_charged_by_xlsx_export_plan() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Rows").unwrap().create_row(10).unwrap();
    let opts = io::WriteOptions {
        max_source_bytes: 4,
        ..Default::default()
    };
    assert!(io::plan_xlsx_export(&workbook, opts).is_err());
}
