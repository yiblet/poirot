//! Independently authored FILTERXML expectations from Microsoft and XPath 1.0;
//! no fixtures were copied or derived from Poirot output. XPath defines string
//! values and order; Microsoft documents arrays, invalid XML and locale parsing.
//! https://support.microsoft.com/en-us/excel/functions/filterxml-function
//! https://learn.microsoft.com/en-us/office/vba/api/excel.worksheetfunction.filterxml
//! https://www.w3.org/TR/1999/REC-xpath-19991116/
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::eval::{
    EvaluationBudgets, EvaluationContext, EvaluationOutcome, EvaluationTarget, RunConfiguration,
    run_formula_with_configuration,
};
use poirot::model::{CellError, CellValue, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

fn evaluate(formula: &str, budgets: EvaluationBudgets) -> EvaluationOutcome<CellValue> {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    run_formula_with_configuration(
        &book,
        0,
        at("Z100").address,
        formula,
        &RunConfiguration::default(),
        budgets,
    )
}

fn value(formula: &str) -> CellValue {
    match evaluate(formula, Default::default()) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{formula}: {other:?}"),
    }
}

#[test]
fn selects_nodes_attributes_positions_and_converts_numeric_text() {
    for (formula, expected) in [
        (
            r#"INDEX(FILTERXML("<r><i>alpha</i><i>beta</i></r>","//i"),2)"#,
            CellValue::Text("beta".into()),
        ),
        (
            r#"FILTERXML("<r><i id='42'>x</i></r>","//i/@id")"#,
            CellValue::Number(42.0),
        ),
        (
            r#"SUM(FILTERXML("<r><i>2.5</i><i>7.5</i></r>","//i"))"#,
            CellValue::Number(10.0),
        ),
        (
            r#"FILTERXML("<r><i>alpha<b>beta</b></i></r>","//i")"#,
            CellValue::Text("alphabeta".into()),
        ),
        (
            r#"FILTERXML("<r><i>first</i><i>last</i></r>","//i[last()]")"#,
            CellValue::Text("last".into()),
        ),
        (
            r#"FILTERXML("<r xmlns:p='urn:x'><p:i>水&amp;é</p:i></r>","//p:i/text()")"#,
            CellValue::Text("水&é".into()),
        ),
        (
            r#"FILTERXML("<r><i/></r>","//i")"#,
            CellValue::Text(String::new()),
        ),
        (
            r#"_xlfn.FILTERXML("<r>ok</r>","/r")"#,
            CellValue::Text("ok".into()),
        ),
    ] {
        assert_eq!(value(formula), expected, "{formula}");
    }
}

#[test]
fn invalid_xml_missing_matches_arity_and_arguments_are_spreadsheet_errors() {
    for (formula, error) in [
        (r#"FILTERXML("<r>","//r")"#, CellError::Value),
        (r#"FILTERXML("<r/>","//missing")"#, CellError::Value),
        (r#"FILTERXML("<r/>","//r[")"#, CellError::Value),
        (r#"FILTERXML("<r/>","//undefined:r")"#, CellError::Value),
        (r#"FILTERXML("<r/>",REPT("x",1025))"#, CellError::Value),
        (r#"FILTERXML()"#, CellError::Value),
        (r#"FILTERXML("<r/>")"#, CellError::Value),
        (r#"FILTERXML("<r/>","//r",1)"#, CellError::Value),
        (r#"FILTERXML(#N/A,"//r")"#, CellError::NA),
        (r#"FILTERXML("<r/>",1/0)"#, CellError::Div0),
    ] {
        assert_eq!(value(formula), CellValue::Error(error), "{formula}");
    }
    assert_eq!(
        value(r#"IFERROR(FILTERXML("broken","//r"),42)"#),
        CellValue::Number(42.0)
    );
}

#[test]
fn backend_limits_and_unsupported_xpath_are_not_catchable_by_iferror() {
    for formula in [
        r#"IFERROR(FILTERXML("<!DOCTYPE r SYSTEM 'file:///etc/passwd'><r/>","//r"),42)"#,
        r#"IFERROR(FILTERXML("<r/>","count(//r)"),42)"#,
        r#"IFERROR(FILTERXML("<r><i/></r>","//r[//i]"),42)"#,
    ] {
        assert!(
            matches!(
                evaluate(formula, Default::default()),
                EvaluationOutcome::Aborted { .. }
            ),
            "{formula}"
        );
    }
    assert!(matches!(
        evaluate(
            r#"IFERROR(FILTERXML("<r>yes</r>","//r"),42)"#,
            EvaluationBudgets {
                run_work: 1,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}

#[test]
fn direct_and_overlay_spills_resize_after_xml_edits() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_value(
            at("A1").address,
            CellValue::Text("<r><i>2</i><i>3</i></r>".into()),
        )
        .unwrap();
    sheet
        .set_formula(at("D1").address, "FILTERXML(A1,\"//i\")")
        .unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(direct.cell(at("D2")).unwrap(), CellValue::Number(3.0));
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        CellValue::Number(3.0)
    );
    overlay
        .set_value(at("A1"), CellValue::Text("<r><i>7</i></r>".into()))
        .unwrap();
    let mut run = overlay.evaluation(&context).unwrap();
    assert_eq!(run.cell(at("D1")).unwrap(), CellValue::Number(7.0));
    assert_eq!(run.cell(at("D2")).unwrap(), CellValue::Blank);
}

#[test]
fn overlay_result_conversion_tracks_numeric_locale_and_date_order() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_formula(at("A1").address, r#"FILTERXML("<r>1,234</r>","/r")"#)
        .unwrap();
    sheet
        .set_formula(at("A2").address, r#"FILTERXML("<r>01/02/2024</r>","/r")"#)
        .unwrap();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let mut context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("A1"))
            .unwrap(),
        CellValue::Number(1234.0)
    );
    // Gregorian date arithmetic against Excel's 1899-12-30 serial epoch.
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("A2"))
            .unwrap(),
        CellValue::Number(45293.0)
    );
    context.number_locale =
        poirot_parsing::number::NumberLocale::new(",", Some("."), Some("€"), "%", ":").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("A1"))
            .unwrap(),
        CellValue::Number(1.234)
    );
    context.localization.date_order = poirot::eval::DateOrder::DayMonthYear;
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("A2"))
            .unwrap(),
        CellValue::Number(45323.0)
    );
}

#[test]
fn xml_operational_errors_preserve_the_backend_cause() {
    use poirot::eval::{EvaluationError, EvaluationReasonError, ResourceFailureKind};
    use poirot_functions::text::xml::XmlQueryError;
    use std::error::Error;
    for (formula, category) in [
        (r#"IFERROR(FILTERXML("<!DOCTYPE r><r/>","/r"),42)"#, 0),
        (r#"IFERROR(FILTERXML("<r/>","count(/r)"),42)"#, 1),
        (
            r#"IFERROR(FILTERXML("<r xmlns:p='a'><i xmlns:p='b'/></r>","/r"),42)"#,
            2,
        ),
        (
            r#"IFERROR(FILTERXML("<r>"&REPT("<i/>",1025)&"</r>","//i"),42)"#,
            3,
        ),
    ] {
        let EvaluationOutcome::Aborted { failure, .. } = evaluate(formula, Default::default())
        else {
            panic!("expected an operational rejection for {formula}")
        };
        let reason = match &failure {
            EvaluationError::UnsupportedCapability { reason, .. } if category < 3 => reason,
            EvaluationError::Resource {
                reason,
                kind: ResourceFailureKind::BackendGuard,
                ..
            } if category == 3 => reason,
            error => panic!("unexpected rejection {error:?}"),
        };
        let EvaluationReasonError::Xml(cause) = reason else {
            panic!("expected XML cause")
        };
        assert!(matches!(
            (category, cause.as_ref()),
            (0, XmlQueryError::DocumentType)
                | (1, XmlQueryError::UnsupportedXPath)
                | (2, XmlQueryError::NamespaceRebinding)
                | (3, XmlQueryError::ResourceLimit)
        ));
        let retained = reason
            .source()
            .unwrap()
            .downcast_ref::<std::sync::Arc<XmlQueryError<EvaluationError>>>()
            .unwrap();
        assert!(std::sync::Arc::ptr_eq(retained, cause));
    }
}
