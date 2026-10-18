//! Strict compatibility entry point over the combined runner's Excel JSON table.
#[test]
fn documented_excel_workbook_cases() {
    if let Err(failures) = poirot_test_runner::json::specs::check_table() {
        panic!("{failures}");
    }
}
