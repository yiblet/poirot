//! Source-backed Excel behavior specifications, including known compatibility gaps.
//! Each test builds a real workbook and observes the public evaluation result.

#[path = "support/mod.rs"]
mod support;

#[test]
fn documented_excel_workbook_cases() {
    support::run_table();
}
