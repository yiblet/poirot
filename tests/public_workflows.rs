//! Exercise the primary workbook -> overlay -> evaluation -> changes workflow.
use poirot::{CellValue, EvaluationContext, Overlay, Spreadsheet, Workbook, eval, io};
fn at(reference: &str) -> eval::EvaluationTarget {
    eval::EvaluationTarget {
        sheet: 0,
        address: reference.parse().unwrap(),
    }
}
#[test]
fn overlay_edits_evaluate_and_apply_without_mutating_the_borrowed_source() {
    let mut model = Workbook::new();
    let sheet = model.add_sheet("Inputs").unwrap();
    sheet.set_value(at("A1").address, 21.).unwrap();
    sheet
        .set_formula(at("B1").address, "A1*2", CellValue::Number(-1.))
        .unwrap();
    let context = EvaluationContext::default();
    let changes = {
        let mut overlay = Overlay::with_workbook(&model).build().unwrap();
        for (input, expected) in [(21., 42.), (24., 48.), (21., 42.)] {
            overlay.set_value(at("A1"), input).unwrap();
            assert_eq!(
                overlay
                    .evaluation(&context)
                    .unwrap()
                    .cell(at("B1"))
                    .unwrap(),
                CellValue::Number(expected)
            );
        }
        assert_eq!(
            model.sheet(0).unwrap().get(at("B1").address).unwrap().value,
            CellValue::Number(-1.)
        );
        overlay.changes()
    };
    model.apply(changes).unwrap();
    let book = Spreadsheet::from_workbook(model);
    let bytes = book
        .plan_xlsx_save(io::WriteOptions::default())
        .unwrap()
        .to_buffer()
        .unwrap();
    let reopened = Spreadsheet::from_bytes(&bytes).unwrap();
    let cell = reopened
        .workbook()
        .sheet(0)
        .unwrap()
        .get(at("B1").address)
        .unwrap();
    assert_eq!(cell.value, CellValue::Number(42.));
    assert_eq!(cell.formula.as_deref(), Some("A1*2"));
}
#[test]
fn default_file_and_byte_imports_retain_the_same_model_and_warnings() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates/poirot-io/tests/fixtures/stale-cache.xlsx");
    let file = Spreadsheet::open(&path).unwrap().into_import_report();
    let bytes = Spreadsheet::from_bytes(&std::fs::read(path).unwrap())
        .unwrap()
        .into_import_report();
    assert_eq!(file.workbook, bytes.workbook);
    assert_eq!(file.warnings, bytes.warnings);
    assert!(Spreadsheet::from_bytes(b"not a workbook").is_err());
}
