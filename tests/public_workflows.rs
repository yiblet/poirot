//! Exercise the primary workbook -> overlay -> evaluation -> changes workflow.
use poirot::calculation::EvaluationContext;
use poirot::eval;
use poirot::eval::overlay::Overlay;
use poirot::{CellValue, Workbook};
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
        .set_formula_with_cached_value(at("B1").address, "A1*2", CellValue::Number(-1.))
        .unwrap();
    let context = EvaluationContext::default();
    let changes = {
        let mut overlay = Overlay::with_workbook(model.model()).build().unwrap();
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
    model.model_mut().apply(changes).unwrap();
    let book = model;
    let bytes = book.save_xlsx().to_buffer().unwrap();
    let reopened = Workbook::from_bytes(&bytes).unwrap();
    let cell = reopened.sheet(0).unwrap().get(at("B1").address).unwrap();
    assert_eq!(cell.value, CellValue::Number(42.));
    assert_eq!(cell.formula.as_deref(), Some("A1*2"));
}
#[test]
fn default_file_and_byte_imports_retain_the_same_model_and_warnings() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates/poirot-io/tests/fixtures/stale-cache.xlsx");
    let file = Workbook::open(&path).unwrap().into_model();
    let bytes = Workbook::from_bytes(&std::fs::read(path).unwrap())
        .unwrap()
        .into_model();
    assert_eq!(file, bytes);
    assert_eq!(file.file_metadata, bytes.file_metadata);
    assert!(Workbook::from_bytes(b"not a workbook").is_err());
}

#[test]
fn evaluated_formats_preserve_precision_text_and_overlay_edits_through_xlsx() {
    use poirot::styles::CellStyle;
    use poirot_functions::format;
    let mut model = Workbook::new();
    let sheet = model.add_sheet("Inputs").unwrap();
    let number = 1234.567890123456;
    sheet.set_value(at("A1").address, number).unwrap();
    sheet.set_formula(at("B1").address, "A1").unwrap();
    sheet
        .set_formula(at("C1").address, "TEXT(A1,\"$0.00\")")
        .unwrap();
    let context = EvaluationContext::default();
    let changes = {
        let mut overlay = Overlay::with_workbook(model.model()).build().unwrap();
        for cell in ["A1", "B1", "C1"] {
            overlay
                .set_style(
                    at(cell),
                    CellStyle {
                        number_format: Some("$#,##0.00".into()),
                        ..Default::default()
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
    model.model_mut().apply(changes).unwrap();
    let book = model;
    let bytes = book.save_xlsx().to_buffer().unwrap();
    let reopened = Workbook::from_bytes(&bytes).unwrap();
    for cell in ["A1", "B1"] {
        let stored = reopened.sheet(0).unwrap().get(at(cell).address).unwrap();
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
        sheet.set_formula(at(cell).address, formula).unwrap();
    }
    let context = EvaluationContext::default();
    let changes = {
        let mut overlay = Overlay::with_workbook(model.model()).build().unwrap();
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
                CellValue::Number(input.unwrap_or(0.))
            );
            assert_eq!(
                evaluation.cell(at("B3")).unwrap(),
                CellValue::Number(input.unwrap_or(0.).min(0.03))
            );
        }
        overlay.changes()
    };
    model.model_mut().apply(changes).unwrap();
    let bytes = model.save_xlsx().to_buffer().unwrap();
    let reopened = Workbook::from_bytes(&bytes).unwrap();
    for (cell, expected) in [
        ("B1", CellValue::Number(0.)),
        ("B2", CellValue::Number(0.03)),
        ("B3", CellValue::Number(0.)),
        ("B4", CellValue::Boolean(true)),
    ] {
        assert_eq!(
            reopened
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

#[test]
fn flushed_sources_export_materialized_formula_and_spill_caches_to_xlsx() {
    let mut model = Workbook::new();
    model
        .add_sheet("S")
        .unwrap()
        .set_value(at("A1").address, 2.)
        .unwrap();
    let mut overlay = Overlay::with_workbook(model.into_model())
        .with_policy(poirot::eval::EvaluationPolicy {
            builtin: eval::BuiltinSemanticsPolicy::ExcelDocumentation,
            arrays: eval::ArrayResultPolicy::DynamicSpill,
            ..Default::default()
        })
        .build()
        .unwrap();
    overlay.set_formula(at("B1"), "A1*3").unwrap();
    overlay.set_formula(at("D1"), "{4,5;6,7}").unwrap();
    let context = EvaluationContext::default();
    overlay.evaluation(&context).unwrap().all().unwrap();
    assert_eq!(overlay.flush().unwrap(), 2);
    let mut saved = overlay.workbook().clone();
    saved.apply(overlay.changes()).unwrap();
    let bytes = Workbook::from_model(saved).save_xlsx().to_buffer().unwrap();
    let reopened = Workbook::from_bytes(&bytes).unwrap();
    let sheet = reopened.sheet(0).unwrap();
    assert_eq!(
        sheet.get(at("B1").address).unwrap().formula.as_deref(),
        Some("A1*3")
    );
    assert_eq!(
        sheet.get(at("B1").address).unwrap().value,
        CellValue::Number(6.)
    );
    assert_eq!(
        sheet.get(at("E2").address).unwrap().value,
        CellValue::Number(7.)
    );
    // File-format spill ownership fidelity is separate from the workspace snapshot;
    // exported visible values must still be correct.
}

#[test]
fn text_placeholder_styles_preserve_values_through_xlsx() {
    use poirot::styles::CellStyle;
    use poirot_functions::format;
    // Microsoft documents @ substitution and Text-format numeric visibility:
    // https://support.microsoft.com/en-us/excel/review-guidelines-for-customizing-a-number-format
    // https://support.microsoft.com/en-us/excel/get-started/available-number-formats-in-excel
    let mut model = Workbook::new();
    let sheet = model.add_sheet("Inputs").unwrap();
    sheet.set_value(at("A1").address, 123.).unwrap();
    sheet.set_value(at("B1").address, "hello").unwrap();
    sheet
        .set_formula(at("C1").address, "TEXT(A1,\"@\")")
        .unwrap();
    for (cell, pattern) in [("A1", "@"), ("B1", "\"Item: \"@")] {
        sheet.get_mut(at(cell).address).unwrap().style = CellStyle {
            number_format: Some(pattern.into()),
            ..Default::default()
        };
    }
    let before = model.clone();
    for book in [
        model.clone(),
        Workbook::from_bytes(&model.save_xlsx().to_buffer().unwrap()).unwrap(),
    ] {
        let context = EvaluationContext::default();
        let mut overlay = Overlay::with_workbook(book.model()).build().unwrap();
        let outputs = overlay
            .evaluation(&context)
            .unwrap()
            .outputs(&[at("A1"), at("B1"), at("C1")])
            .unwrap();
        for (index, expected) in ["123", "Item: hello"].iter().enumerate() {
            let output = &outputs[index];
            let pattern = output
                .formatting
                .as_ref()
                .unwrap()
                .number_format
                .as_deref()
                .unwrap();
            let plan = format::compile_format(pattern, Default::default()).unwrap();
            assert_eq!(
                &format::render_value(&output.value, &plan, Default::default())
                    .unwrap()
                    .text,
                expected
            );
        }
        assert_eq!(outputs[0].value, CellValue::Number(123.));
        assert_eq!(outputs[1].value, CellValue::Text("hello".into()));
        assert_eq!(outputs[2].value, CellValue::Text("123".into()));
        assert_eq!(
            book.sheet(0).unwrap().get(at("A1").address).unwrap().value,
            CellValue::Number(123.)
        );
        assert_eq!(
            book.sheet(0).unwrap().get(at("B1").address).unwrap().value,
            CellValue::Text("hello".into())
        );
    }
    assert_eq!(model, before);
}

#[test]
fn integer_mask_text_and_styles_preserve_numbers_through_xlsx() {
    use poirot::styles::CellStyle;
    use poirot_functions::format;
    // Microsoft documents digit separators for stored numbers and TEXT results:
    // https://support.microsoft.com/en-us/excel/keeping-leading-zeros-and-large-numbers
    // Independent inputs below partition decimal digits from the right.
    let mut model = Workbook::new();
    let sheet = model.add_sheet("Inputs").unwrap();
    let rows = [
        ("A1", "B1", 24681357., "000-00-0000", "024-68-1357"),
        ("A2", "B2", 2468135790., "(000) 000-0000", "(246) 813-5790"),
        ("A3", "B3", 12345., "00000-0000", "00001-2345"),
    ];
    for (input, result, value, pattern, _) in rows {
        sheet.set_value(at(input).address, value).unwrap();
        sheet.get_mut(at(input).address).unwrap().style = CellStyle {
            number_format: Some(pattern.into()),
            ..Default::default()
        };
        sheet
            .set_formula(at(result).address, format!("TEXT({input},\"{pattern}\")"))
            .unwrap();
    }
    let before = model.clone();
    for book in [
        model.clone(),
        Workbook::from_bytes(&model.save_xlsx().to_buffer().unwrap()).unwrap(),
    ] {
        let context = EvaluationContext::default();
        let mut overlay = Overlay::with_workbook(book.model()).build().unwrap();
        for (input, result, value, pattern, expected) in rows {
            let outputs = overlay
                .evaluation(&context)
                .unwrap()
                .outputs(&[at(input), at(result)])
                .unwrap();
            assert_eq!(outputs[0].value, CellValue::Number(value));
            assert_eq!(outputs[1].value, CellValue::Text(expected.into()));
            let saved_pattern = outputs[0]
                .formatting
                .as_ref()
                .unwrap()
                .number_format
                .as_deref()
                .unwrap();
            assert_eq!(saved_pattern, pattern);
            let plan = format::compile_format(saved_pattern, Default::default()).unwrap();
            assert_eq!(
                format::render_value(&outputs[0].value, &plan, Default::default())
                    .unwrap()
                    .text,
                expected
            );
            assert_eq!(
                book.sheet(0).unwrap().get(at(input).address).unwrap().value,
                CellValue::Number(value)
            );
        }
    }
    assert_eq!(model, before);
}
