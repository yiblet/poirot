//! Independently authored examples of Microsoft's CELL contract:
//! https://support.microsoft.com/en-us/excel/functions/cell-function
//! No upstream code, algorithms or fixtures are reused.
use poirot::eval::overlay::Overlay;
use poirot::eval::{
    EvaluationContext, EvaluationOutcome, EvaluationTarget, RunConfiguration,
    run_formula_with_configuration,
};
use poirot::model::{CellError, CellStyle, CellValue, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

fn book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1", 42.).unwrap();
    sheet.set_value("A2", "hello").unwrap();
    sheet.set_value("A3", true).unwrap();
    sheet.set_formula("A4", "1/0").unwrap();
    book
}

fn run(book: &Workbook, formula: &str) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        book,
        0,
        at("Z1").address,
        formula,
        &RunConfiguration::default(),
        Default::default(),
    )
}

fn value(book: &Workbook, formula: &str) -> CellValue {
    match run(book, formula) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{formula}: {other:?}"),
    }
}

#[test]
fn explicit_reference_coordinates_contents_and_types() {
    let mut book = book();
    book.define_name("Named", "S!A2", None).unwrap();
    for (formula, expected) in [
        ("CELL(\"address\",A2:B4)", CellValue::Text("$A$2".into())),
        ("CELL(\"row\",INDEX(A1:A4,3))", 3.0.into()),
        ("CELL(\"col\",INDIRECT(\"C6\"))", 3.0.into()),
        ("CELL(\"CONTENTS\",Named)", "hello".into()),
        ("CELL(\"contents\",B1)", 0.0.into()),
        ("CELL(\"type\",B1)", "b".into()),
        ("CELL(\"type\",A1)", "v".into()),
        ("CELL(\"type\",A2)", "l".into()),
        ("CELL(\"type\",A3)", "v".into()),
        ("CELL(\"type\",A4)", "v".into()),
        ("CELL(\"contents\",A4)", CellValue::Error(CellError::Div0)),
        ("CELL(\"row\",A4)", 4.0.into()),
        ("CELL(\"prefix\",A2)", "'".into()),
        ("CELL(\"prefix\",A1)", "".into()),
        ("CELL(\"unknown\",A1)", CellValue::Error(CellError::Value)),
        ("CELL(\"row\",12)", CellValue::Error(CellError::Value)),
        ("CELL(\"row\",#REF!)", CellValue::Error(CellError::Ref)),
        ("IF(FALSE,CELL(\"filename\",A1),42)", 42.0.into()),
    ] {
        assert_eq!(value(&book, formula), expected, "{formula}");
    }
}

#[test]
fn documented_number_formats_and_same_value_style_edits() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("B1", "CELL(\"format\",A1)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    for (format, expected) in [
        ("General", "G"),
        ("0.00", "F2"),
        ("0%", "P0"),
        ("m/d/yy", "D4"),
        ("$#,##0_);[Red]($#,##0)", "C0-"),
    ] {
        overlay
            .set_style(
                at("A1"),
                CellStyle {
                    number_format: Some(format.into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("B1"))
                .unwrap(),
            CellValue::Text(expected.into())
        );
    }
    overlay.set_value(at("A1"), 42.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B1"))
            .unwrap(),
        CellValue::Text("C0-".into())
    );
    for (request, expected) in [("color", 1.), ("parentheses", 0.)] {
        let source = format!("CELL(\"{request}\",A1)");
        overlay.set_formula(at("B2"), source).unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("B2"))
                .unwrap(),
            CellValue::Number(expected)
        );
    }
}

#[test]
fn contents_recalculate_through_incremental_edits() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("B1", "CELL(\"contents\",A1)")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("B2", "CELL(\"type\",A1)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    for replacement in [
        CellValue::Number(1.),
        CellValue::Text("one".into()),
        CellValue::Blank,
    ] {
        overlay.set_value(at("A1"), replacement.clone()).unwrap();
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(
            run.cell(at("B1")).unwrap(),
            if replacement == CellValue::Blank {
                0.0.into()
            } else {
                replacement.clone()
            }
        );
        assert_eq!(
            run.cell(at("B2")).unwrap(),
            CellValue::Text(
                match replacement {
                    CellValue::Blank => "b",
                    CellValue::Text(_) => "l",
                    _ => "v",
                }
                .into()
            )
        );
    }
}

#[test]
fn absent_application_metadata_and_limits_are_uncatchable() {
    let book = book();
    for formula in [
        "IFERROR(CELL(\"row\"),99)",
        "IFERROR(CELL(\"row\",),99)",
        "IFERROR(CELL(\"filename\",A1),99)",
        "CELL(\"width\",A1)",
        "CELL(\"protect\",A1)",
    ] {
        assert!(
            matches!(
                run(&book, formula),
                EvaluationOutcome::Aborted {
                    failure: poirot::eval::EvaluationError::UnsupportedCapability {
                        reason: poirot::eval::EvaluationReasonError::CellSelectedUiReference
                            | poirot::eval::EvaluationReasonError::CellUnmodeledMetadata,
                        ..
                    },
                    ..
                }
            ),
            "{formula}"
        );
    }
    let outcome = run_formula_with_configuration(
        &book,
        0,
        at("Z1").address,
        "IFERROR(CELL(\"address\",A1),99)",
        &RunConfiguration::default(),
        poirot::eval::EvaluationBudgets {
            text_bytes: 1,
            ..Default::default()
        },
    );
    assert!(
        matches!(outcome, EvaluationOutcome::Aborted { .. }),
        "{outcome:?}"
    );
}

#[test]
fn alignment_reads_effective_visual_style() {
    use poirot::model::{Alignment, HorizontalAlignment, VisualStyle};
    use std::sync::Arc;
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("B1", "CELL(\"prefix\",A2)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    for (alignment, expected) in [
        (HorizontalAlignment::Left, "'"),
        (HorizontalAlignment::Right, "\""),
        (HorizontalAlignment::Center, "^"),
        (HorizontalAlignment::Fill, "\\"),
    ] {
        overlay
            .set_style(
                at("A2"),
                CellStyle {
                    visual: Some(Arc::new(VisualStyle {
                        alignment: Some(Alignment {
                            horizontal: Some(alignment),
                            ..Default::default()
                        }),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("B1"))
                .unwrap(),
            CellValue::Text(expected.into())
        );
    }
}

#[test]
fn direct_style_requests_reject_unclassified_custom_formats() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_style(
            at("A1").address,
            CellStyle {
                number_format: Some("0.00%".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(value(&book, "CELL(\"format\",A1)"), "P2".into());
    assert_eq!(value(&book, "CELL(\"color\",A1)"), 0.0.into());
    assert_eq!(value(&book, "CELL(\"parentheses\",A1)"), 0.0.into());
    book.sheet_mut(0)
        .unwrap()
        .set_style(
            at("A1").address,
            CellStyle {
                number_format: Some("[Blue]0.000".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(matches!(
        run(&book, "IFERROR(CELL(\"format\",A1),99)"),
        EvaluationOutcome::Aborted {
            failure: poirot::eval::EvaluationError::UnsupportedCapability {
                reason: poirot::eval::EvaluationReasonError::CellCustomNumberFormat,
                ..
            },
            ..
        }
    ));
}
