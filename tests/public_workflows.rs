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

#[test]
fn evaluated_formats_preserve_precision_text_and_overlay_edits_through_xlsx() {
    use poirot::{CellStyle, format};
    let mut model = Workbook::new();
    let sheet = model.add_sheet("Inputs").unwrap();
    let number = 1234.567890123456;
    sheet.set_value(at("A1").address, number).unwrap();
    sheet
        .set_formula(at("B1").address, "A1", CellValue::Blank)
        .unwrap();
    sheet
        .set_formula(at("C1").address, "TEXT(A1,\"$0.00\")", CellValue::Blank)
        .unwrap();
    let context = EvaluationContext::default();
    let changes = {
        let mut overlay = Overlay::with_workbook(&model).build().unwrap();
        for cell in ["A1", "B1", "C1"] {
            overlay
                .set_style(
                    at(cell),
                    CellStyle {
                        number_format: Some("$#,##0.00".into()),
                    },
                )
                .unwrap();
        }
        let outputs = overlay
            .evaluation(&context)
            .unwrap()
            .outputs(&[at("A1"), at("B1"), at("C1"), at("D1")])
            .unwrap();
        for output in &outputs[..2] {
            assert_eq!(output.value, CellValue::Number(number));
            let plan = format::compile_format(
                output
                    .formatting
                    .as_ref()
                    .unwrap()
                    .number_format
                    .as_deref()
                    .unwrap(),
                Default::default(),
            )
            .unwrap();
            assert_eq!(
                format::render_value(&output.value, &plan, Default::default())
                    .unwrap()
                    .text,
                "$1,234.57"
            );
        }
        assert_eq!(outputs[2].value, CellValue::Text("$1234.57".into()));
        assert_eq!(outputs[3].formatting, None);
        assert_eq!(
            overlay.evaluation(&context).unwrap().all().unwrap(),
            outputs[..3]
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
    for cell in ["A1", "B1"] {
        let stored = reopened
            .workbook()
            .sheet(0)
            .unwrap()
            .get(at(cell).address)
            .unwrap();
        assert_eq!(stored.value, CellValue::Number(number));
        assert_eq!(stored.style.number_format.as_deref(), Some("$#,##0.00"));
    }
}

#[test]
fn lookup_blank_consumers_survive_overlay_reset_materialization_and_xlsx() {
    let mut model = Workbook::new();
    let sheet = model.add_sheet("Inputs").unwrap();
    sheet.set_value(at("A1").address, "KY").unwrap();
    for (cell, formula) in [
        ("B1", "HLOOKUP(\"KY\",A1:A2,2,FALSE)"),
        ("B2", "MIN(HLOOKUP(\"KY\",A1:A2,2,FALSE),0.03)"),
        ("B3", "MIN(B1,0.03)"),
        ("B4", "ISBLANK(HLOOKUP(\"KY\",A1:A2,2,FALSE))"),
    ] {
        sheet
            .set_formula(at(cell).address, formula, CellValue::Blank)
            .unwrap();
    }
    let context = EvaluationContext::default();
    let changes = {
        let mut overlay = Overlay::with_workbook(&model).build().unwrap();
        for input in [None, Some(0.), Some(0.02), None] {
            overlay.reset();
            if let Some(value) = input {
                overlay.set_value(at("A2"), value).unwrap();
            }
            let mut evaluation = overlay.evaluation(&context).unwrap();
            let expected = input.unwrap_or(0.03).min(0.03);
            assert_eq!(
                evaluation.cell(at("B2")).unwrap(),
                CellValue::Number(expected)
            );
            assert_eq!(
                evaluation.cell(at("B4")).unwrap(),
                CellValue::Boolean(input.is_none())
            );
            assert_eq!(
                evaluation.cell(at("B1")).unwrap(),
                input.map_or(CellValue::Blank, CellValue::Number)
            );
            assert_eq!(
                evaluation.cell(at("B3")).unwrap(),
                CellValue::Number(expected)
            );
        }
        overlay.changes()
    };
    model.apply(changes).unwrap();
    let bytes = Spreadsheet::from_workbook(model)
        .plan_xlsx_save(io::WriteOptions::default())
        .unwrap()
        .to_buffer()
        .unwrap();
    let reopened = Spreadsheet::from_bytes(&bytes).unwrap();
    for (cell, expected) in [
        ("B1", CellValue::Blank),
        ("B2", CellValue::Number(0.03)),
        ("B3", CellValue::Number(0.03)),
        ("B4", CellValue::Boolean(true)),
    ] {
        assert_eq!(
            reopened
                .workbook()
                .sheet(0)
                .unwrap()
                .get(at(cell).address)
                .unwrap()
                .value,
            expected,
            "{cell}"
        );
    }
}
