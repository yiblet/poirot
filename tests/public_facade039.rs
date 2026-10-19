use poirot::{CellValue, Workbook, io, xls};

#[test]
fn saved_typed_formula_evidence_and_import_losses_remain_discoverable() {
    let mut book = Workbook::new();
    let sheet = book.model_mut().add_sheet("Data").unwrap();
    sheet
        .set_formula("A1".parse().unwrap(), "1=1", CellValue::Boolean(true))
        .unwrap();
    for bytes in [
        book.save_xlsx().to_buffer().unwrap(),
        book.save_xls().to_buffer().unwrap(),
    ] {
        let reopened = Workbook::load().from_bytes(&bytes).unwrap();
        let cell = reopened
            .model()
            .sheet(0)
            .unwrap()
            .get("A1".parse().unwrap())
            .unwrap();
        assert_eq!(cell.value, CellValue::Boolean(true));
        assert!(cell.formula.is_some());
        assert!(
            !reopened
                .file_metadata
                .as_ref()
                .unwrap()
                .import_warnings
                .as_slice()
                .is_empty()
        );
        let warnings = reopened
            .file_metadata
            .as_ref()
            .unwrap()
            .import_warnings
            .as_slice()
            .to_vec();
        assert_eq!(
            reopened.into_model().file_metadata.unwrap().import_warnings,
            warnings
        );
    }
}

#[test]
fn pure_planning_preserves_source_and_budget_failure_has_typed_cause() {
    use std::error::Error;
    let mut book = Workbook::new();
    book.model_mut()
        .add_sheet("Data")
        .unwrap()
        .set_value("A1".parse().unwrap(), "payload")
        .unwrap();
    let before = book.model().clone();
    let options = io::WriteOptions {
        max_source_bytes: 0,
        ..Default::default()
    };
    let error = book.save_xlsx().options(options).build().unwrap_err();
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<io::IoError>()
            .is_some()
    );
    assert_eq!(book.model(), &before);
    let plan = book.save_xls().build().unwrap();
    assert!(matches!(plan, poirot::SavePlan::Xls(_)));
    assert_eq!(book.model(), &before);
    let error = book
        .save_xls()
        .policy(xls::XlsExportPolicy {
            max_cells: 0,
            ..Default::default()
        })
        .build()
        .unwrap_err();
    assert!(matches!(error, poirot::WorkbookError::Xls(_)));
    assert_eq!(book.model(), &before);
}

fn formula_book(formulas: &[(&str, &str)]) -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.model_mut().add_sheet("Data").unwrap();
    for (address, formula) in formulas {
        sheet
            .set_formula(
                address.parse().unwrap(),
                *formula,
                CellValue::Number(-999.0),
            )
            .unwrap();
    }
    book
}
fn target(reference: &str) -> poirot::eval::EvaluationTarget {
    poirot::eval::EvaluationTarget {
        sheet: 0,
        address: reference.parse().unwrap(),
    }
}
#[test]
fn overlay_recursively_invalidates_and_applies_owned_cached_changes() {
    let book = formula_book(&[("B1", "A1+1"), ("C1", "B1+1"), ("D1", "8")]);
    let mut overlay = poirot::Overlay::with_workbook(book.model())
        .build()
        .unwrap();
    let context = poirot::EvaluationContext::default();
    overlay.set_value(target("A1"), 1.).unwrap();
    overlay.evaluation(&context).unwrap().all().unwrap();
    assert_eq!(
        *overlay.cached_value(target("C1")).unwrap(),
        CellValue::Number(3.)
    );
    overlay.set_value(target("A1"), 4.).unwrap();
    assert!(overlay.cached_value(target("C1")).is_none());
    assert!(overlay.cached_value(target("D1")).is_some());
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(
        evaluation.cell(target("C1")).unwrap(),
        CellValue::Number(6.)
    );
    assert_eq!(evaluation.usage().formula_executions, 2);
    assert!(!evaluation.result_changes().unwrap().is_empty());
}
#[test]
fn volatile_cells_are_reexecuted_for_each_evaluation_instance() {
    let book = formula_book(&[("A1", "RAND()"), ("B1", "A1+1"), ("C1", "8")]);
    let context = poirot::EvaluationContext::default();
    let mut overlay = poirot::Overlay::with_workbook(book.model())
        .build()
        .unwrap();
    overlay.evaluation(&context).unwrap().all().unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    evaluation.outputs(&[target("B1"), target("C1")]).unwrap();
    assert_eq!(evaluation.usage().formula_executions, 2);
    assert_eq!(evaluation.usage().external_cache_hits, 1);
}
#[test]
fn formatting_context_and_text_policy_invalidate_actual_consumers() {
    let book = formula_book(&[("A1", "DOLLAR(1,2)"), ("B1", r#"LEN("😀")"#), ("C1", "8")]);
    let mut overlay = poirot::Overlay::with_workbook(book.model())
        .with_policy(poirot::OverlayPolicy {
            text: poirot::eval::TextSemanticsPolicy {
                source: poirot::eval::TextSourceVersion::Poi7321209,
            },
            ..Default::default()
        })
        .build()
        .unwrap();
    let mut context = poirot::EvaluationContext::default();
    let mut currency = poirot::eval::CurrencyContext {
        decimal_separator: ".".into(),
        grouping_separator: ",".into(),
        primary_group: 3,
        secondary_group: 3,
        positive_prefix: "$".into(),
        positive_suffix: "".into(),
        negative_prefix: "($".into(),
        negative_suffix: ")".into(),
    };
    context.formatting = poirot::eval::FormattingConfiguration::with_currency(currency.clone());
    overlay.evaluation(&context).unwrap().all().unwrap();
    // The explicit POI contract counts the emoji's two UTF16 units.
    assert_eq!(
        *overlay.cached_value(target("B1")).unwrap(),
        CellValue::Number(2.)
    );
    currency.positive_prefix = "USD ".into();
    context.formatting = poirot::eval::FormattingConfiguration::with_currency(currency);
    let mut evaluation = overlay.evaluation(&context).unwrap();
    let outputs = evaluation
        .outputs(&[target("A1"), target("B1"), target("C1")])
        .unwrap();
    assert_eq!(outputs[0].value, CellValue::Text("USD 1.00".into()));
    assert_eq!(evaluation.usage().formula_executions, 1);
    drop(evaluation);
    let mut policy = overlay.policy().clone();
    policy.text.source = poirot::eval::TextSourceVersion::ExcelCompatibilityV2Documented;
    overlay.set_policy(policy).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    let outputs = evaluation
        .outputs(&[target("A1"), target("B1"), target("C1")])
        .unwrap();
    assert_eq!(outputs[1].value, CellValue::Number(1.));
    assert_eq!(evaluation.usage().formula_executions, 1);
}
#[test]
fn budget_failure_preserves_source_and_previously_completed_cache() {
    let book = formula_book(&[("A1", "1+2")]);
    let before = book.model().clone();
    let mut overlay = poirot::Overlay::with_workbook(book.model())
        .build()
        .unwrap();
    overlay
        .evaluation(&poirot::EvaluationContext::default())
        .unwrap()
        .all()
        .unwrap();
    let context = poirot::EvaluationContext {
        budgets: poirot::eval::EvaluationBudgets {
            run_work: 0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert!(evaluation.cell(target("A1")).is_err());
    assert!(evaluation.result_changes().unwrap().is_empty());
    drop(evaluation);
    assert_eq!(book.model(), &before);
    assert_eq!(
        *overlay.cached_value(target("A1")).unwrap(),
        CellValue::Number(3.)
    );
}
#[test]
fn empty_rows_and_defined_names_flow_through_overlay_evaluation() {
    let mut book = Workbook::new();
    let sheet = book.model_mut().add_sheet("Data").unwrap();
    sheet.create_row(5).unwrap();
    sheet.set_value("A1".parse().unwrap(), 4.).unwrap();
    sheet
        .set_formula("B1".parse().unwrap(), "Rate*A1", CellValue::Blank)
        .unwrap();
    book.model_mut().define_name("Rate", "2", None).unwrap();
    let mut overlay = poirot::Overlay::with_workbook(book.model())
        .build()
        .unwrap();
    assert_eq!(
        overlay
            .evaluation(&poirot::EvaluationContext::default())
            .unwrap()
            .cell(target("B1"))
            .unwrap(),
        CellValue::Number(8.)
    );
    assert!(book.model().sheet(0).unwrap().row(5).is_some());
}

#[test]
fn unsupported_signature_and_xls_formula_keep_typed_error_chains() {
    use std::error::Error;
    let error = Workbook::load().from_bytes(b"unsupported").unwrap_err();
    assert!(matches!(
        error.source().unwrap().downcast_ref::<io::IoError>(),
        Some(io::IoError::Unsupported(_))
    ));
    let mut book = Workbook::new();
    for name in ["Sheet1", "Sheet2", "Sheet3"] {
        book.model_mut().add_sheet(name).unwrap();
    }
    book.model_mut()
        .sheet_mut(0)
        .unwrap()
        .set_formula(
            "A1".parse().unwrap(),
            "SUM(Sheet1:Sheet3!A1)",
            CellValue::Number(-999.0),
        )
        .unwrap();
    let saved = book.model().clone();
    let error = book.save_xls().build().unwrap_err();
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<xls::XlsError>()
            .is_some()
    );
    assert_eq!(book.model(), &saved);
}

#[test]
fn serialization_failure_preserves_existing_destination() {
    use std::error::Error;
    let book = formula_book(&[("A1", "1+2")]);
    let save = book.save_xlsx().options(io::WriteOptions {
        max_output_bytes: 0,
        ..Default::default()
    });
    let path = std::env::temp_dir().join(format!(
        "poirot-facade039-preserve-{}.xlsx",
        std::process::id()
    ));
    std::fs::write(&path, b"existing destination").unwrap();
    let error = save.write_file(&path).unwrap_err();
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<io::IoError>()
            .is_some()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"existing destination");
    std::fs::remove_file(path).unwrap();
}
