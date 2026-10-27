//! Coordinate and spelling expectations follow Microsoft's function documentation:
//! https://support.microsoft.com/en-us/excel/functions/row-function
//! https://support.microsoft.com/en-us/excel/functions/column-function
//! https://support.microsoft.com/en-us/excel/functions/rows-function
//! https://support.microsoft.com/en-us/excel/functions/columns-function
//! https://support.microsoft.com/en-us/excel/functions/areas-function
//! https://support.microsoft.com/en-us/excel/functions/address-function
//! https://support.microsoft.com/en-us/excel/functions/sheet-function
//! Coordinate and dimension expectations are calculated from those documented rules.
//! HYPERLINK forwarding, poison-cell laziness and budget failures exercise Poirot's
//! existing adapter service contracts; forwarding reference identity does not claim
//! native Excel hyperlink-navigation or validation compatibility. No upstream code
//! or fixtures are reused.
use poirot::eval::{EvaluationBudgets, EvaluationError, EvaluationOutcome, run_formula};
use poirot::model::{CellError, CellValue, Workbook};

/// Evaluate one public expression with a caller position away from its source.
fn value(workbook: &Workbook, formula: &str) -> CellValue {
    match run_formula(
        workbook,
        0,
        "Z7".parse().unwrap(),
        formula,
        Default::default(),
        Default::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("{formula}: {outcome:?}"),
    }
}

#[test]
fn intact_reference_metadata_never_reads_source_values() {
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Data")
        .unwrap()
        .set_formula("B2", "UNKNOWN_FUNCTION()")
        .unwrap();
    workbook.add_sheet("Other").unwrap();
    workbook.add_sheet("Third").unwrap();
    for (formula, expected) in [
        ("ROWS(B2:D4)", 3.),
        ("COLUMNS(B2:D4)", 3.),
        ("SUM(ROW(B2:D4))", 9.),
        ("SUM(COLUMN(B2:D4))", 9.),
        ("ROWS(ROW(B2:D4))", 3.),
        ("COLUMNS(COLUMN(B2:D4))", 3.),
        ("ROWS(COLUMN(B2:D4))", 1.),
        ("COLUMNS(ROW(B2:D4))", 1.),
        ("ROW()", 7.),
        ("COLUMN()", 26.),
        ("AREAS((B2:D4,F6,H8:I9))", 3.),
        ("SHEET(Other!B2)", 2.),
        // Poirot retains a sheet span as one area and resolves its first sheet.
        ("AREAS(Data:Third!B2)", 1.),
        ("SHEET(Other:Third!B2)", 2.),
        ("SHEET(\"Other\")", 2.),
        ("SHEET()", 1.),
        ("SHEET(UNICHAR(79)&\"ther\")", 2.),
        ("ROWS({1,2;3,4})", 2.),
        ("COLUMNS({1,2;3,4})", 2.),
        ("AREAS(HYPERLINK(UNKNOWN_FUNCTION(),B2:D4))", 1.),
        ("ROWS(HYPERLINK(UNKNOWN_FUNCTION(),B2:D4))", 3.),
    ] {
        assert_eq!(
            value(&workbook, formula),
            CellValue::Number(expected),
            "{formula}"
        );
    }
    for formula in ["ROWS(B:B)", "COLUMNS(2:2)"] {
        assert!(
            matches!(
                run_formula(
                    &workbook,
                    0,
                    "Z7".parse().unwrap(),
                    formula,
                    Default::default(),
                    EvaluationBudgets {
                        materialized_values: 4,
                        ..Default::default()
                    }
                ),
                EvaluationOutcome::Complete { .. }
            ),
            "{formula}"
        );
    }
}

#[test]
fn address_shared_broadcast_preserves_omissions_and_axis_modes() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Data").unwrap();
    for (formula, expected) in [
        ("ADDRESS(3,28)", "$AB$3"),
        ("ADDRESS(3,28,2)", "AB$3"),
        ("ADDRESS(3,28,3)", "$AB3"),
        ("ADDRESS(3,28,4)", "AB3"),
        ("ADDRESS(3,28,,FALSE)", "R3C28"),
        ("ADDRESS(3,28,4,FALSE)", "R[3]C[28]"),
        ("ADDRESS(3,28,,,\"O'Brien\")", "'O''Brien'!$AB$3"),
        ("CONCAT(ADDRESS({2;3},{1,2},4))", "A2B2A3B3"),
        ("ADDRESS(1048576,16384)", "$XFD$1048576"),
        ("ADDRESS(1,1,,,\"XFD1\")", "'XFD1'!$A$1"),
    ] {
        assert_eq!(
            value(&workbook, formula),
            CellValue::Text(expected.into()),
            "{formula}"
        );
    }
    for formula in [
        "ADDRESS(0,1)",
        "ADDRESS(1,16385)",
        "ADDRESS(1048577,1)",
        "ADDRESS(1,1,0)",
        "ADDRESS(1,1,A1)",
    ] {
        assert_eq!(
            value(&workbook, formula),
            CellValue::Error(CellError::Value),
            "{formula}"
        );
    }
    assert_eq!(
        value(&workbook, "ADDRESS(1,1,,1/0)"),
        CellValue::Error(CellError::Div0)
    );
}

#[test]
fn metadata_rejects_unions_and_scalar_coordinates_but_propagates_errors() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Data").unwrap();
    for formula in [
        "ROW((B2,C3))",
        "COLUMN({1,2})",
        "ROWS((B2,C3))",
        "AREAS(1)",
        "ROW(@A1)",
        "ROWS(A1)",
    ] {
        let expected = if formula == "ROWS(A1)" {
            CellValue::Number(1.)
        } else {
            CellValue::Error(CellError::Value)
        };
        assert_eq!(value(&workbook, formula), expected, "{formula}");
    }
    for formula in ["ROWS(1/0)", "COLUMN(1/0)", "AREAS(1/0)", "SHEET(1/0)"] {
        assert_eq!(
            value(&workbook, formula),
            CellValue::Error(CellError::Div0),
            "{formula}"
        );
    }
    assert_eq!(value(&workbook, "@ROW(B2:D4)"), CellValue::Number(2.));
    assert_eq!(value(&workbook, "@COLUMN(B2:D4)"), CellValue::Number(2.));
}

#[test]
fn axis_publication_limits_and_selected_operational_failures_survive_iferror() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Data").unwrap();
    for formula in [
        "IFERROR(SUM(ROW(A:A)),99)",
        "IFERROR(HYPERLINK(0,INDIRECT(\"[missing.xlsx]Data!A1\")),99)",
    ] {
        let outcome = run_formula(
            &workbook,
            0,
            "Z7".parse().unwrap(),
            formula,
            Default::default(),
            EvaluationBudgets {
                materialized_values: 32,
                ..Default::default()
            },
        );
        assert!(
            matches!(
                outcome,
                EvaluationOutcome::Aborted {
                    failure: EvaluationError::Budget(_)
                        | EvaluationError::UnsupportedCapability { .. },
                    ..
                }
            ),
            "{formula}: {outcome:?}"
        );
    }
}

#[test]
fn utf16_labels_are_forwarded_and_sheet_projection_keeps_supplementary_text() {
    use poirot::model::ExcelText;
    let mut workbook = Workbook::new();
    let isolated = CellValue::TextUtf16(ExcelText::from_units(vec![0xD800]));
    workbook
        .add_sheet("Data")
        .unwrap()
        .set_value("A1", isolated.clone())
        .unwrap();
    workbook.add_sheet("Other😀").unwrap();
    assert_eq!(value(&workbook, "HYPERLINK(0,A1)"), isolated);
    assert_eq!(
        value(&workbook, "SHEET(\"Other\"&UNICHAR(128512))"),
        CellValue::Number(2.)
    );
    assert_eq!(
        value(&workbook, "ADDRESS(1,1,,,UNICHAR(128512))"),
        CellValue::Text("'😀'!$A$1".into())
    );
    // Existing text-projection limit: ADDRESS needs Unicode sheet spelling;
    // isolated UTF-16 is forwarded losslessly only by display-value selection.
    assert!(matches!(
        run_formula(
            &workbook,
            0,
            "Z7".parse().unwrap(),
            "ADDRESS(1,1,,,A1)",
            Default::default(),
            Default::default()
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationError::UnsupportedCapability { .. },
            ..
        }
    ));
}
