//! Public controls for the execution services shared by regular functions.
//! SUM and COUNT operand rules follow Microsoft's function documentation:
//! https://support.microsoft.com/en-us/office/sum-function-043e1c7d-7726-4e80-8f32-07b23e057f89
//! https://support.microsoft.com/en-us/office/count-function-a59cd7fc-b623-4d93-87a4-d23bf411294c
use poirot::eval::{
    BudgetKind, EvaluationBudgets, EvaluationError, EvaluationInputs, EvaluationOutcome,
    run_formula, run_recalculation,
};
use poirot::model::{CellError, CellValue, Workbook};

fn book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1", "4").unwrap();
    sheet.set_value("A2", true).unwrap();
    sheet.set_value("A3", 6.).unwrap();
    book
}

fn evaluate(
    book: &Workbook,
    formula: &str,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula(
        book,
        0,
        "Z1".parse().unwrap(),
        formula,
        EvaluationInputs::default(),
        budgets,
    )
}

fn complete(book: &Workbook, formula: &str) -> CellValue {
    match evaluate(book, formula, EvaluationBudgets::default()) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{formula}: {other:?}"),
    }
}

#[test]
fn aggregate_admission_retains_direct_reference_and_array_origins() {
    let book = book();
    for (formula, expected) in [
        ("SUM(\"4\",TRUE,6)", 11.),
        ("SUM(A1:A3)", 6.),
        ("SUM({\"4\",TRUE,6})", 6.),
        ("COUNT(\"4\",TRUE,6)", 3.),
        ("COUNT(A1:A3)", 1.),
        ("COUNT({\"4\",TRUE,6})", 1.),
    ] {
        assert_eq!(
            complete(&book, formula),
            CellValue::Number(expected),
            "{formula}"
        );
    }
}

#[test]
fn omitted_argument_is_distinct_from_a_blank_reference() {
    let book = book();
    // SUMPRODUCT's documented array operands include blank reference members as zero.
    // An omitted operand is an invalid argument rather than a blank cell.
    // https://support.microsoft.com/en-us/office/sumproduct-function-16753e75-9f68-487a-9a3e-1e750a4d3c04
    assert_eq!(complete(&book, "SUMPRODUCT(B1,2)"), CellValue::Number(0.));
    assert_eq!(
        complete(&book, "SUMPRODUCT(,2)"),
        CellValue::Error(CellError::Value)
    );
}

#[test]
fn ordered_spreadsheet_errors_and_eager_operational_failures_stay_distinct() {
    let mut book = book();
    for (formula, error) in [
        ("SUM(#REF!,#DIV/0!)", CellError::Ref),
        ("SUM(#DIV/0!,#REF!)", CellError::Div0),
    ] {
        assert_eq!(complete(&book, formula), CellValue::Error(error));
    }
    book.sheet_mut(0)
        .unwrap()
        .set_formula("A4", "SUM(T:S!B1)")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_value("A3", CellValue::Error(CellError::Ref))
        .unwrap();
    book.add_sheet("T").unwrap();
    assert!(matches!(
        evaluate(&book, "SUM(A3:A4)", EvaluationBudgets::default()),
        EvaluationOutcome::Aborted {
            failure: EvaluationError::UnsupportedCapability { .. },
            ..
        }
    ));
}

#[test]
fn lazy_branch_avoids_work_and_repeated_volatile_reference_uses_one_draw() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("B1", "RAND()")
        .unwrap();
    assert_eq!(complete(&book, "IF(FALSE,1/0,7)"), CellValue::Number(7.));
    let skipped = evaluate(&book, "IF(FALSE,B1,7)", EvaluationBudgets::default());
    assert!(matches!(
        skipped,
        EvaluationOutcome::Complete {
            value: CellValue::Number(7.),
            ..
        }
    ));
    assert!(skipped.usage().random_accesses.is_empty());
    let reused = evaluate(&book, "B1-B1", EvaluationBudgets::default());
    assert!(matches!(
        reused,
        EvaluationOutcome::Complete {
            value: CellValue::Number(0.),
            ..
        }
    ));
    assert_eq!(reused.usage().random_accesses.len(), 1);
    assert_eq!(reused.usage().random_accesses[0].draws, 1);
}

#[test]
fn budget_abort_does_not_publish_partial_recalculation() {
    let mut book = book();
    for (cell, formula, cached) in [("B1", "1+1", 101.), ("B2", "REPT(\"x\",100)", 102.)] {
        book.sheet_mut(0)
            .unwrap()
            .set_formula_with_cached_value(cell, formula, CellValue::Number(cached))
            .unwrap();
    }
    let before = book.clone();
    assert!(matches!(
        run_recalculation(
            &mut book,
            EvaluationInputs::default(),
            EvaluationBudgets {
                text_bytes: 50,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationError::Budget(ref failure),
            ..
        } if failure.kind == BudgetKind::TextBytes
    ));
    assert_eq!(book, before);
}
