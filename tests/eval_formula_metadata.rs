//! Independently authored expectations from Microsoft's function documentation:
//! https://support.microsoft.com/en-us/excel/functions/formulatext-function
//! https://support.microsoft.com/en-us/excel/functions/isformula-function
//! https://support.microsoft.com/en-us/excel/functions/sheets-function
//! No upstream code, fixtures or algorithms are reused.
use poirot::eval::overlay::Overlay;
use poirot::eval::{
    EvaluationContext, EvaluationOutcome, EvaluationTarget, RunConfiguration,
    run_formula_with_configuration,
};
use poirot::model::{CellError, CellValue, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

fn book() -> Workbook {
    let mut book = Workbook::new();
    for name in ["First", "Second", "Third"] {
        book.add_sheet(name).unwrap();
    }
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_formula("A1", "=1/0").unwrap();
    sheet.set_value("A2", 1.).unwrap();
    sheet.set_formula("B1", "=FORMULATEXT(B1)").unwrap();
    sheet.set_formula("B2", "ISFORMULA(B2)").unwrap();
    book
}

fn evaluate(book: &Workbook, formula: &str) -> CellValue {
    match run_formula_with_configuration(
        book,
        0,
        at("Z1").address,
        formula,
        &RunConfiguration::default(),
        Default::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        result => panic!("{formula}: {result:?}"),
    }
}

#[test]
fn inspects_authored_source_without_executing_errors_or_self_references() {
    let book = book();
    for (formula, expected) in [
        ("FORMULATEXT(A1)", CellValue::Text("=1/0".into())),
        ("FORMULATEXT(A1:B2)", CellValue::Text("=1/0".into())),
        (
            "FORMULATEXT(INDEX(A1:B2,1,1))",
            CellValue::Text("=1/0".into()),
        ),
        (
            "FORMULATEXT(INDIRECT(\"A1\"))",
            CellValue::Text("=1/0".into()),
        ),
        (
            "FORMULATEXT(B1)",
            CellValue::Text("=FORMULATEXT(B1)".into()),
        ),
        ("ISFORMULA(A1)", CellValue::Boolean(true)),
        ("ISFORMULA(A2)", CellValue::Boolean(false)),
        ("ISFORMULA(B2)", CellValue::Boolean(true)),
        ("ISFORMULA(C1)", CellValue::Boolean(false)),
        ("INDEX(ISFORMULA(A1:A2),2,1)", CellValue::Boolean(false)),
        ("IF(FALSE,FORMULATEXT(1/0),42)", CellValue::Number(42.)),
    ] {
        assert_eq!(evaluate(&book, formula), expected, "{formula}");
    }
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    let mut run = overlay.evaluation(&context).unwrap();
    assert_eq!(
        run.cell(at("B1")).unwrap(),
        CellValue::Text("=FORMULATEXT(B1)".into())
    );
    assert_eq!(run.cell(at("B2")).unwrap(), CellValue::Boolean(true));
}

#[test]
fn rejects_invalid_types_and_propagates_reference_errors() {
    let book = book();
    for (formula, error) in [
        ("FORMULATEXT(A2)", CellError::NA),
        ("FORMULATEXT(C1)", CellError::NA),
        ("FORMULATEXT(42)", CellError::Value),
        ("FORMULATEXT({1,2})", CellError::Value),
        ("ISFORMULA(42)", CellError::Value),
        ("ISFORMULA({1,2})", CellError::Value),
        ("FORMULATEXT(#REF!)", CellError::Ref),
        ("ISFORMULA(#REF!)", CellError::Ref),
        ("FORMULATEXT()", CellError::Value),
        ("ISFORMULA(A1,A2)", CellError::Value),
        ("SHEETS(42)", CellError::Ref),
        ("SHEETS(\"First\")", CellError::Ref),
        ("SHEETS(#REF!)", CellError::Ref),
        ("SHEETS(A1,A2)", CellError::Value),
    ] {
        assert_eq!(
            evaluate(&book, formula),
            CellValue::Error(error),
            "{formula}"
        );
    }
}

#[test]
fn counts_worksheets_and_reference_spans_without_evaluating_members() {
    let mut book = book();
    book.define_name("Span", "Second:Third!A1", None).unwrap();
    book.define_name("FormulaCell", "First!A1", None).unwrap();
    book.sheet_mut(1)
        .unwrap()
        .set_presentation(poirot::model::SheetPresentation {
            visibility: poirot::model::SheetVisibility::Hidden,
            ..Default::default()
        });
    assert_eq!(
        evaluate(&book, "FORMULATEXT(FormulaCell)"),
        CellValue::Text("=1/0".into())
    );
    assert_eq!(evaluate(&book, "SHEETS(Span)"), CellValue::Number(2.));
    for (formula, expected) in [
        ("SHEETS()", 3.),
        ("SHEETS(A1)", 1.),
        ("SHEETS(First:Third!A1)", 3.),
        ("SHEETS(Second:Third!A1:B2)", 2.),
    ] {
        assert_eq!(
            evaluate(&book, formula),
            CellValue::Number(expected),
            "{formula}"
        );
    }
    book.add_sheet("Fourth").unwrap();
    assert_eq!(evaluate(&book, "SHEETS()"), CellValue::Number(4.));
}

#[test]
fn effective_source_edits_invalidate_metadata_even_when_values_are_equal() {
    let mut book = book();
    book.sheet_mut(0).unwrap().set_formula("A1", "1+1").unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("C1", "FORMULATEXT(A1)")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("C2", "ISFORMULA(A1)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    for (source, text, is_formula) in [
        (Some("1+1"), "=1+1", true),
        (Some("2"), "=2", true),
        (None, "", false),
        (Some("SUM(1,1)"), "=SUM(1,1)", true),
    ] {
        if let Some(source) = source {
            overlay.set_formula(at("A1"), source).unwrap();
        } else {
            overlay.set_value(at("A1"), 2.).unwrap();
        }
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(
            run.cell(at("C1")).unwrap(),
            if is_formula {
                CellValue::Text(text.into())
            } else {
                CellValue::Error(CellError::NA)
            }
        );
        assert_eq!(run.cell(at("C2")).unwrap(), CellValue::Boolean(is_formula));
    }
}

#[test]
fn enforces_formula_bar_length_in_utf16_units() {
    let mut book = book();
    for (source, expected) in [
        (format!("\"{}\"", "a".repeat(8189)), true),
        (format!("\"{}\"", "a".repeat(8190)), false),
        (format!("\"{}\"", "😀".repeat(4095)), false),
    ] {
        book.sheet_mut(0)
            .unwrap()
            .set_formula("A1", &source)
            .unwrap();
        assert_eq!(
            evaluate(&book, "FORMULATEXT(A1)"),
            if expected {
                CellValue::Text(format!("={source}"))
            } else {
                CellValue::Error(CellError::NA)
            }
        );
    }
}

#[test]
fn metadata_is_lazy_and_operational_limits_escape_iferror() {
    let mut book = book();
    // This target cannot execute successfully, but its authored source is inspectable.
    book.sheet_mut(0)
        .unwrap()
        .set_formula("A1", "INDIRECT(\"[missing.xlsx]S!A1\")")
        .unwrap();
    assert_eq!(evaluate(&book, "ISFORMULA(A1)"), CellValue::Boolean(true));
    assert_eq!(
        evaluate(&book, "FORMULATEXT(A1)"),
        CellValue::Text("=INDIRECT(\"[missing.xlsx]S!A1\")".into())
    );
    let outcome = run_formula_with_configuration(
        &book,
        0,
        at("Z1").address,
        "IFERROR(FORMULATEXT(A1),99)",
        &RunConfiguration::default(),
        poirot::eval::EvaluationBudgets {
            text_bytes: 16,
            ..Default::default()
        },
    );
    assert!(
        matches!(outcome, EvaluationOutcome::Aborted { .. }),
        "{outcome:?}"
    );
}

#[test]
fn generated_spill_children_have_no_authored_formula() {
    use poirot::eval::{
        ArrayResultPolicy, EvaluationRun, FormulaCompiler, PreparationPolicy, WorkbookView,
    };
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("D1", "{1;2}")
        .unwrap();
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&book, &policy);
    let inspect = compiler.compile_formula("ISFORMULA(D2)", &policy);
    let text = compiler.compile_formula("FORMULATEXT(D2)", &policy);
    let configuration =
        RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::DynamicSpill);
    let mut run = EvaluationRun::new_with_configuration(
        WorkbookView::new(&book, &plan, &policy).unwrap(),
        &configuration,
        Default::default(),
    )
    .unwrap();
    assert_eq!(run.evaluate_cell(at("D1")).unwrap(), CellValue::Number(1.));
    assert_eq!(run.evaluate_cell(at("D2")).unwrap(), CellValue::Number(2.));
    assert_eq!(
        run.evaluate_formula(at("Z1"), &inspect).unwrap(),
        CellValue::Boolean(false)
    );
    assert_eq!(
        run.evaluate_formula(at("Z1"), &text).unwrap(),
        CellValue::Error(CellError::NA)
    );
}
