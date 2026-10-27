//! Public controls for checked statistical aggregation.
//! Expected sample and population results use the definitions documented by Microsoft:
//! https://support.microsoft.com/en-us/excel/functions/var-function
//! https://support.microsoft.com/en-us/excel/statistical-functions-reference
//! https://support.microsoft.com/en-us/excel/calculate-the-median-of-a-group-of-numbers
use poirot::eval::{EvaluationBudgets, EvaluationError, EvaluationOutcome, run_formula};
use poirot::model::{CellError, CellValue, Workbook};

fn outcome(book: &Workbook, formula: &str) -> EvaluationOutcome<CellValue> {
    run_formula(
        book,
        0,
        "Z1".parse().unwrap(),
        formula,
        Default::default(),
        Default::default(),
    )
}

#[test]
fn numeric_samples_keep_direct_and_reference_admission() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1", 1.).unwrap();
    sheet.set_value("A2", "ignored").unwrap();
    sheet.set_value("A3", 3.).unwrap();
    for (formula, expected) in [
        ("MEDIAN(A1:A3)", 2.),
        ("VAR(A1:A3)", 2.),
        ("VARP(A1:A3)", 1.),
        ("STDEVP(A1:A3)", 1.),
        ("MEDIAN(1,\"3\",5)", 3.),
        ("SUMSQ(A1:A3)", 10.),
    ] {
        assert!(
            matches!(outcome(&book, formula), EvaluationOutcome::Complete { value: CellValue::Number(value), .. } if value == expected),
            "{formula}"
        );
    }
}

#[test]
fn eager_reference_reads_abort_before_reporting_earlier_spreadsheet_error() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_value("A1", CellValue::Error(CellError::Ref))
        .unwrap();
    sheet.set_formula("A2", "SUM(T:S!B1)").unwrap();
    book.add_sheet("T").unwrap();
    assert!(matches!(
        outcome(&book, "MEDIAN(A1:A2)"),
        EvaluationOutcome::Aborted {
            failure: EvaluationError::UnsupportedCapability { .. },
            ..
        }
    ));
    assert!(matches!(
        outcome(&book, "COUNT(A1:A2)"),
        EvaluationOutcome::Aborted {
            failure: EvaluationError::UnsupportedCapability { .. },
            ..
        }
    ));
}

#[test]
fn ordered_sheet_span_stops_at_the_first_spreadsheet_error() {
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_value("A1", CellValue::Error(CellError::Div0))
        .unwrap();
    book.add_sheet("T")
        .unwrap()
        .set_formula("A1", "SUM(T:S!B1)")
        .unwrap();
    assert!(matches!(
        outcome(&book, "MEDIAN(S:T!A1)"),
        EvaluationOutcome::Complete {
            value: CellValue::Error(CellError::Div0),
            ..
        }
    ));
}

#[test]
fn ignored_blank_references_do_not_grow_numeric_sample_budget() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    let result = run_formula(
        &book,
        0,
        "Z1".parse().unwrap(),
        "MEDIAN(A1,A2,A3)",
        Default::default(),
        EvaluationBudgets {
            materialized_values: 1,
            ..Default::default()
        },
    );
    assert!(
        matches!(
            result,
            EvaluationOutcome::Complete {
                value: CellValue::Error(CellError::Num),
                ..
            }
        ),
        "{result:?}"
    );
}
