//! Ordinary workbook edits and calculation need no evaluator workspace or change batch.
use poirot::{
    CalculationError, Cell, CellAddress, CellError, CellStyle, CellValue, EvaluationContext,
    FileFormat, FileMetadata, ImportDiagnostic, ImportDiagnosticKind, ModelError, Workbook,
    Worksheet, eval,
};
use std::error::Error;

fn at(address: &str) -> CellAddress {
    address.parse().unwrap()
}
fn value(book: &Workbook, sheet: usize, address: &str) -> CellValue {
    book.sheet(sheet)
        .unwrap()
        .get(at(address))
        .unwrap()
        .value
        .clone()
}
fn inputs() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Inputs").unwrap();
    sheet.set_value(at("A1"), 21.).unwrap();
    sheet
        .set_formula_with_cached_value(at("B1"), "A1*2", CellValue::Number(-1.))
        .unwrap();
    sheet.set_formula(at("C1"), "B1+1").unwrap();
    book
}

#[test]
fn recalculate_replaces_saved_values_after_each_input_change() {
    let mut book = inputs();
    book.recalculate().unwrap();
    // Multiplication and addition, rather than saved caches, determine these results.
    assert_eq!(value(&book, 0, "B1"), CellValue::Number(42.));
    assert_eq!(value(&book, 0, "C1"), CellValue::Number(43.));
    book.sheet_mut(0).unwrap().set_value(at("A1"), 24.).unwrap();
    book.recalculate().unwrap();
    assert_eq!(value(&book, 0, "B1"), CellValue::Number(48.));
    assert_eq!(value(&book, 0, "C1"), CellValue::Number(49.));
    assert_eq!(
        book.sheet(0)
            .unwrap()
            .get(at("B1"))
            .unwrap()
            .formula
            .as_deref(),
        Some("A1*2")
    );
}

#[test]
fn calculated_results_survive_xls_and_xlsx_buffers() {
    let mut book = inputs();
    book.recalculate().unwrap();
    for bytes in [
        book.save_xlsx().to_buffer().unwrap(),
        book.save_xls().to_buffer().unwrap(),
    ] {
        let loaded = Workbook::from_bytes(&bytes).unwrap();
        assert_eq!(value(&loaded, 0, "B1"), CellValue::Number(42.));
        assert_eq!(value(&loaded, 0, "C1"), CellValue::Number(43.));
        assert_eq!(
            loaded
                .sheet(0)
                .unwrap()
                .get(at("B1"))
                .unwrap()
                .formula
                .as_deref(),
            Some("A1*2")
        );
    }
}

#[test]
fn recalculation_preserves_metadata_and_cell_styles() {
    let mut book = inputs();
    let metadata = FileMetadata {
        format: FileFormat::Xlsx,
        import_warnings: vec![ImportDiagnostic {
            kind: ImportDiagnosticKind::ExternalLink,
            detail: "external link omitted".into(),
        }],
    };
    book.file_metadata = Some(metadata.clone());
    book.sheet_mut(0)
        .unwrap()
        .get_mut(at("B1"))
        .unwrap()
        .style
        .number_format = Some("0.00".into());
    book.recalculate().unwrap();
    assert_eq!(book.file_metadata, Some(metadata));
    assert_eq!(
        book.sheet(0)
            .unwrap()
            .get(at("B1"))
            .unwrap()
            .style
            .number_format
            .as_deref(),
        Some("0.00")
    );
}

#[test]
fn spreadsheet_errors_are_saved_values_not_operational_failures() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Errors").unwrap();
    sheet.set_formula(at("A1"), "1/0").unwrap();
    sheet.set_formula(at("B1"), "IFERROR(A1,7)").unwrap();
    book.recalculate().unwrap();
    // Excel division by zero produces #DIV/0!; IFERROR catches spreadsheet errors.
    assert_eq!(value(&book, 0, "A1"), CellValue::Error(CellError::Div0));
    assert_eq!(value(&book, 0, "B1"), CellValue::Number(7.));
}

#[test]
fn operational_abort_does_not_publish_earlier_results() {
    let mut book = inputs();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("D1"), "REPT(\"x\",64)")
        .unwrap();
    let before = book.clone();
    let context = EvaluationContext {
        budgets: eval::EvaluationBudgets {
            text_bytes: 8,
            ..Default::default()
        },
        ..Default::default()
    };
    let error = book.recalculate_with_context(context).unwrap_err();
    assert!(matches!(error, CalculationError::Evaluation(_)));
    assert!(error.source().is_some());
    assert_eq!(book, before);
    // A later unrestricted call is a fresh calculation, independent of the abort.
    book.recalculate().unwrap();
    assert_eq!(value(&book, 0, "B1"), CellValue::Number(42.));
    assert_eq!(value(&book, 0, "D1"), CellValue::Text("x".repeat(64)));
}

#[test]
fn invalid_calculation_settings_do_not_mutate_the_workbook() {
    let mut book = inputs();
    let before = book.clone();
    let context = EvaluationContext {
        budgets: eval::EvaluationBudgets {
            depth: 129,
            ..Default::default()
        },
        ..Default::default()
    };
    let error = book.recalculate_with_context(context).unwrap_err();
    assert!(matches!(error, CalculationError::Preparation(_)));
    assert!(error.source().is_some());
    assert_eq!(book, before);
}

#[test]
fn explicit_context_controls_clock_and_reproducible_randomness() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Clock").unwrap();
    sheet.set_formula(at("A1"), "TODAY()").unwrap();
    sheet.set_formula(at("B1"), "RAND()").unwrap();
    let context = EvaluationContext {
        as_of_time: std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_609_459_200),
        randomness_seed: 42,
        localization: eval::Localization {
            utc_offset_seconds: 0,
            date_order: eval::DateOrder::MonthDayYear,
        },
        ..Default::default()
    };
    book.recalculate_with_context(context.clone()).unwrap();
    // 2021-01-01 is day 44197 in Excel's 1900 date system, including its leap-day convention.
    assert_eq!(value(&book, 0, "A1"), CellValue::Number(44197.));
    let CellValue::Number(random) = value(&book, 0, "B1") else {
        panic!("RAND must return a number")
    };
    assert!((0.0..1.0).contains(&random));
    book.recalculate_with_context(context).unwrap();
    assert_eq!(value(&book, 0, "B1"), CellValue::Number(random));
}

#[test]
fn recalculate_publishes_spills_and_clears_stale_children() {
    let mut book = Workbook::new();
    book.add_sheet("Arrays")
        .unwrap()
        .set_formula(at("A1"), "{1,2;3,4}")
        .unwrap();
    book.recalculate().unwrap();
    for (address, expected) in [("A1", 1.), ("B1", 2.), ("A2", 3.), ("B2", 4.)] {
        assert_eq!(value(&book, 0, address), CellValue::Number(expected));
    }
    // Direct source mutation is public too: recalculation must not preserve an old footprint.
    book.sheet_mut(0)
        .unwrap()
        .get_mut(at("A1"))
        .unwrap()
        .formula = Some("9".into());
    book.recalculate().unwrap();
    assert_eq!(value(&book, 0, "A1"), CellValue::Number(9.));
    for address in ["B1", "A2", "B2"] {
        assert!(book.sheet(0).unwrap().get(at(address)).is_none());
    }
}

#[test]
fn ordinary_formula_setter_clears_cache_and_preserves_style() {
    let mut sheet = Worksheet::new("Inputs").unwrap();
    let style = CellStyle {
        number_format: Some("0.00".into()),
    };
    let mut cell = Cell::new(17.);
    cell.style = style.clone();
    sheet.set(at("A1"), cell).unwrap();
    sheet.set_formula(at("A1"), " = 1+2 ").unwrap();
    let cell = sheet.get(at("A1")).unwrap();
    assert_eq!(cell.formula.as_deref(), Some("1+2"));
    assert_eq!(cell.value, CellValue::Blank);
    assert_eq!(cell.style, style);
    let before = sheet.clone();
    let error: ModelError = sheet.set_formula(at("A1"), " = ").unwrap_err();
    assert!(!error.to_string().is_empty());
    assert_eq!(sheet, before);
    assert!(
        sheet
            .set_formula(
                CellAddress {
                    row: u32::MAX,
                    col: 0
                },
                "1"
            )
            .is_err()
    );
    assert_eq!(sheet, before);
    sheet
        .set_formula_with_cached_value(at("A1"), "4+5", CellValue::Number(999.))
        .unwrap();
    assert_eq!(sheet.get(at("A1")).unwrap().value, CellValue::Number(999.));
    assert_eq!(sheet.get(at("A1")).unwrap().style, style);
}

#[test]
fn rename_updates_quoted_references_and_defined_names() {
    let mut book = Workbook::new();
    book.add_sheet("Old data")
        .unwrap()
        .set_value(at("A1"), 21.)
        .unwrap();
    book.add_sheet("Results")
        .unwrap()
        .set_formula(at("A1"), "'Old data'!A1*2")
        .unwrap();
    book.define_name("Input", "'Old data'!A1", None).unwrap();
    book.sheet_mut(1)
        .unwrap()
        .set_formula(at("B1"), "Input+1")
        .unwrap();
    let outcome = book.rename_sheet(0, "New's data").unwrap();
    assert_eq!(outcome.changed_sources.len(), 2);
    assert_eq!(book.sheet(0).unwrap().name(), "New's data");
    assert_eq!(
        book.sheet(1)
            .unwrap()
            .get(at("A1"))
            .unwrap()
            .formula
            .as_deref(),
        Some("'New''s data'!A1*2")
    );
    assert_eq!(
        book.defined_name("Input", 1).unwrap().formula,
        "'New''s data'!A1"
    );
    book.recalculate().unwrap();
    assert_eq!(value(&book, 1, "A1"), CellValue::Number(42.));
    assert_eq!(value(&book, 1, "B1"), CellValue::Number(22.));
}

#[test]
fn rename_failures_leave_names_formulas_and_values_unchanged() {
    let mut book = inputs();
    book.add_sheet("Taken").unwrap();
    for (sheet, name) in [(0, "Taken"), (0, "bad/name"), (10, "New")] {
        let before = book.clone();
        assert!(book.rename_sheet(sheet, name).is_err());
        assert_eq!(book, before);
    }
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("D1"), "SUM(")
        .unwrap();
    let before = book.clone();
    assert!(book.rename_sheet(0, "New").is_err());
    assert_eq!(book, before);
}

#[test]
fn named_cell_read_distinguishes_missing_cells_and_stored_blanks() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Customer's quote").unwrap();
    sheet.set_value(at("A1"), CellValue::Blank).unwrap();
    sheet.set_value(at("B1"), 25.).unwrap();

    assert_eq!(
        book.cell("Customer's quote", at("A1")).unwrap().value,
        CellValue::Blank
    );
    assert_eq!(
        book.cell("CUSTOMER'S QUOTE", at("B1")).unwrap().value,
        CellValue::Number(25.)
    );
    assert!(book.cell("Customer's quote", at("C1")).is_none());
    assert!(book.cell("Missing", at("A1")).is_none());
}

#[test]
fn named_cell_read_returns_stored_formula_results_until_recalculation() {
    let mut book = inputs();
    assert_eq!(
        book.cell("Inputs", at("B1")).unwrap().value,
        CellValue::Number(-1.)
    );
    book.recalculate().unwrap();
    book.sheet_by_name_mut("Inputs")
        .unwrap()
        .set_value(at("A1"), 24.)
        .unwrap();
    // Reading does not evaluate: the previous 21 * 2 result remains until calculation.
    assert_eq!(
        book.cell("Inputs", at("B1")).unwrap().value,
        CellValue::Number(42.)
    );
    book.recalculate().unwrap();
    assert_eq!(
        book.cell("Inputs", at("B1")).unwrap().value,
        CellValue::Number(48.)
    );
}
