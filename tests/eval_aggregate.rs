//! AGGREGATE contracts follow Microsoft's selector/option tables and reference rules:
//! https://support.microsoft.com/en-us/excel/functions/aggregate-function
//! https://learn.microsoft.com/en-us/office/vba/api/excel.worksheetfunction.aggregate
//! Expected reductions below are independently calculated; no live Excel oracle is claimed.
use poirot::eval::{
    EvaluationBudgets, EvaluationError, EvaluationOutcome, RunConfiguration,
    run_formula_with_configuration,
};
use poirot::model::{CellError, CellValue, RowVisibility, Workbook};

fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    for (address, value) in [("A1", 2.), ("A2", 4.), ("A3", 4.), ("A4", 6.)] {
        sheet
            .set_value(
                address.parse::<poirot::model::CellAddress>().unwrap(),
                value,
            )
            .unwrap();
    }
    workbook
}

fn outcome(
    workbook: &Workbook,
    formula: &str,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        workbook,
        0,
        "Z1".parse::<poirot::model::CellAddress>().unwrap(),
        formula,
        &RunConfiguration::default(),
        budgets,
    )
}

fn value(workbook: &Workbook, formula: &str) -> CellValue {
    match outcome(workbook, formula, EvaluationBudgets::default()) {
        EvaluationOutcome::Complete { value, .. } => value,
        result => panic!("{formula}: {result:?}"),
    }
}

#[test]
fn all_nineteen_selectors_use_their_documented_operation() {
    let workbook = workbook();
    // Sum 16, mean 4, centered sum of squares 8, product 192.
    for (selector, k, expected) in [
        (1, None, 4.),
        (2, None, 4.),
        (3, None, 4.),
        (4, None, 6.),
        (5, None, 2.),
        (6, None, 192.),
        (7, None, (8.0f64 / 3.).sqrt()),
        (8, None, 2.0f64.sqrt()),
        (9, None, 16.),
        (10, None, 8. / 3.),
        (11, None, 2.),
        (12, None, 4.),
        (13, None, 4.),
        (14, Some(2.), 4.),
        (15, Some(2.), 4.),
        (16, Some(0.25), 3.5),
        (17, Some(1.), 3.5),
        (18, Some(0.25), 2.5),
        (19, Some(1.), 2.5),
    ] {
        let extra = k.map(|k| format!(",{k}")).unwrap_or_default();
        let formula = format!("AGGREGATE({selector},0,A1:A4{extra})");
        let CellValue::Number(actual) = value(&workbook, &formula) else {
            panic!("{formula}")
        };
        assert!(
            (actual - expected).abs() < 1e-12,
            "{formula}: {actual} != {expected}"
        );
    }
}

fn exclusions() -> Workbook {
    let mut workbook = workbook();
    let sheet = workbook.sheet_mut(0).unwrap();
    sheet
        .set_value("A2".parse::<poirot::model::CellAddress>().unwrap(), 3.)
        .unwrap();
    sheet
        .set_value("A3".parse::<poirot::model::CellAddress>().unwrap(), 5.)
        .unwrap();
    sheet
        .set_value("B1".parse::<poirot::model::CellAddress>().unwrap(), 7.)
        .unwrap();
    sheet
        .set_value("B2".parse::<poirot::model::CellAddress>().unwrap(), 11.)
        .unwrap();
    sheet
        .set_formula(
            "A4".parse::<poirot::model::CellAddress>().unwrap(),
            "SUBTOTAL(9,B1)",
        )
        .unwrap();
    sheet
        .set_formula(
            "A5".parse::<poirot::model::CellAddress>().unwrap(),
            "AGGREGATE(9,4,B2)",
        )
        .unwrap();
    sheet
        .set_value(
            "A6".parse::<poirot::model::CellAddress>().unwrap(),
            CellValue::Error(CellError::NA),
        )
        .unwrap();
    sheet
        .set_row_visibility(
            1,
            RowVisibility {
                manually_hidden: true,
                filtered_out: false,
            },
        )
        .unwrap();
    sheet
        .set_row_visibility(
            2,
            RowVisibility {
                manually_hidden: false,
                filtered_out: true,
            },
        )
        .unwrap();
    workbook
}

#[test]
fn all_options_control_errors_hidden_rows_and_nested_totals_independently() {
    let workbook = exclusions();
    // Filtered rows follow Poirot's existing SUBTOTAL convention for every
    // option; this aspect has not been verified against live Excel.
    for (option, expected) in [
        (0, CellValue::Error(CellError::NA)),
        (1, CellValue::Error(CellError::NA)),
        (2, CellValue::Number(5.)),
        (3, CellValue::Number(2.)),
        (4, CellValue::Error(CellError::NA)),
        (5, CellValue::Error(CellError::NA)),
        (6, CellValue::Number(23.)),
        (7, CellValue::Number(20.)),
    ] {
        let formula = format!("AGGREGATE(9,{option},A1:A6)");
        assert_eq!(value(&workbook, &formula), expected, "{formula}");
    }
    assert_eq!(
        value(&workbook, "AGGREGATE(9,,A1:A5)"),
        CellValue::Number(5.)
    );
    assert_eq!(
        value(&workbook, "AGGREGATE(14,3,A1:A5,1)"),
        CellValue::Number(2.)
    );
    // Calculated arrays have no row/nested-total metadata: sorted 2,3,5,7,11.
    assert_eq!(
        value(&workbook, "AGGREGATE(14,3,A1:A5+0,1)"),
        CellValue::Number(11.)
    );
}

#[test]
fn count_and_counta_preserve_their_distinct_error_and_blank_contracts() {
    let mut workbook = workbook();
    let sheet = workbook.sheet_mut(0).unwrap();
    sheet
        .set_value("A2".parse::<poirot::model::CellAddress>().unwrap(), true)
        .unwrap();
    sheet
        .set_value("A3".parse::<poirot::model::CellAddress>().unwrap(), "")
        .unwrap();
    sheet
        .set_value("A4".parse::<poirot::model::CellAddress>().unwrap(), "3")
        .unwrap();
    sheet
        .set_value(
            "A5".parse::<poirot::model::CellAddress>().unwrap(),
            CellValue::Error(CellError::Div0),
        )
        .unwrap();
    for (formula, expected) in [
        ("AGGREGATE(2,4,A1:A6)", 1.),
        ("AGGREGATE(2,6,A1:A6)", 1.),
        ("AGGREGATE(3,4,A1:A6)", 5.),
        ("AGGREGATE(3,6,A1:A6)", 4.),
        ("AGGREGATE(9,6,A1:A6)", 2.),
    ] {
        assert_eq!(value(&workbook, formula), CellValue::Number(expected));
    }
}

#[test]
fn scalar_controls_reference_forms_and_k_are_validated() {
    let mut workbook = workbook();
    workbook.add_sheet("T").unwrap();
    assert_eq!(
        value(&workbook, "AGGREGATE(9.9,0.9,(A1:A2,A3:A4))"),
        CellValue::Number(16.)
    );
    assert_eq!(
        value(&workbook, "AGGREGATE(9,0,A1:A2,A3:A4)"),
        CellValue::Number(16.)
    );
    assert_eq!(
        value(&workbook, "AGGREGATE(14,0,{1,2,3},2)"),
        CellValue::Number(2.)
    );
    for formula in [
        "AGGREGATE(0,0,A1)",
        "AGGREGATE(20,0,A1)",
        "AGGREGATE(9,8,A1)",
        "AGGREGATE(9,-1,A1)",
        "AGGREGATE(9,0,1)",
        "AGGREGATE(9,0,{1,2})",
        "AGGREGATE(14,0,A1:A4)",
        "AGGREGATE(14,0,A1:A4,1,2)",
        "AGGREGATE(9,0,S:T!A1)",
        "AGGREGATE(14,0,S:T!A1,1)",
    ] {
        assert_eq!(
            value(&workbook, formula),
            CellValue::Error(CellError::Value),
            "{formula}"
        );
    }
    for formula in [
        "AGGREGATE(14,0,A1:A4,0)",
        "AGGREGATE(15,0,A1:A4,5)",
        "AGGREGATE(16,0,A1:A4,1.1)",
        "AGGREGATE(18,0,A1:A4,0)",
    ] {
        assert_eq!(
            value(&workbook, formula),
            CellValue::Error(CellError::Num),
            "{formula}"
        );
    }
    assert_eq!(
        value(&workbook, "AGGREGATE(9,0,A7:A8)"),
        CellValue::Number(0.)
    );
    assert_eq!(
        value(&workbook, "AGGREGATE(1,0,A7:A8)"),
        CellValue::Error(CellError::Div0)
    );
    assert_eq!(
        value(&workbook, "AGGREGATE(14,0,A1:A4,1/0)"),
        CellValue::Error(CellError::Div0)
    );
    assert_eq!(
        value(&workbook, "IF(TRUE,7,AGGREGATE(9,0,A1/0))"),
        CellValue::Number(7.)
    );
}

#[test]
fn excluded_formulas_are_not_evaluated_and_operational_refusals_cannot_be_caught() {
    let mut workbook = exclusions();
    let sheet = workbook.sheet_mut(0).unwrap();
    sheet
        .set_formula("A2".parse::<poirot::model::CellAddress>().unwrap(), "1/0")
        .unwrap();
    sheet
        .set_formula(
            "A4".parse::<poirot::model::CellAddress>().unwrap(),
            "IF(TRUE,1/0,SUBTOTAL(9,B1))",
        )
        .unwrap();
    sheet
        .set_formula(
            "A5".parse::<poirot::model::CellAddress>().unwrap(),
            "IF(TRUE,1/0,AGGREGATE(9,4,B2))",
        )
        .unwrap();
    assert_eq!(
        value(&workbook, "AGGREGATE(9,1,A1:A5)"),
        CellValue::Number(2.)
    );
    assert!(matches!(
        outcome(
            &workbook,
            "IFERROR(AGGREGATE(9,3,A1:A6),99)",
            EvaluationBudgets {
                materialized_values: 2,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationError::Budget(_),
            ..
        }
    ));
}

#[test]
fn nested_formula_metadata_reads_remain_subject_to_text_budgets() {
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(
            "A4".parse::<poirot::model::CellAddress>().unwrap(),
            format!("AGGREGATE(9,4,A1)+{}1", " ".repeat(100)),
        )
        .unwrap();
    assert!(matches!(
        outcome(
            &workbook,
            "IFERROR(AGGREGATE(9,2,A4),99)",
            EvaluationBudgets {
                text_bytes: 50,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationError::Budget(_),
            ..
        }
    ));
}

#[test]
fn incremental_edits_reclassify_nested_formulas_and_row_changes_invalidate_plans() {
    use poirot::eval::overlay::Overlay;
    use poirot::eval::{
        EvaluationContext, EvaluationTarget, FormulaCompiler, PreparationPolicy, WorkbookView,
    };
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(
            "B1".parse::<poirot::model::CellAddress>().unwrap(),
            "AGGREGATE(9,0,A1:A4)",
        )
        .unwrap();
    let at = |address: &str| EvaluationTarget {
        sheet: 0,
        address: address.parse::<poirot::model::CellAddress>().unwrap(),
    };
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B1"))
            .unwrap(),
        CellValue::Number(16.)
    );
    overlay.set_formula(at("A2"), "AGGREGATE(9,4,A3)").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B1"))
            .unwrap(),
        CellValue::Number(12.)
    );
    overlay.set_formula(at("A2"), "SUBTOTAL(9,A3)").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B1"))
            .unwrap(),
        CellValue::Number(12.)
    );
    overlay.set_value(at("A2"), 10.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B1"))
            .unwrap(),
        CellValue::Number(22.)
    );
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_row_visibility(
            0,
            RowVisibility {
                manually_hidden: false,
                filtered_out: true,
            },
        )
        .unwrap();
    assert!(matches!(
        WorkbookView::new(&workbook, &plan, &policy),
        Err(EvaluationError::StalePlan)
    ));
    assert_eq!(
        value(&workbook, "AGGREGATE(9,4,A1:A4)"),
        CellValue::Number(14.)
    );
}

#[test]
fn array_ranks_lift_over_one_sample_and_keep_per_member_errors() {
    // The sample 2,4,4,6 is shared by all four ranks; invalid rank zero affects
    // only its own output. Microsoft's array-formula guide demonstrates rank
    // arguments broadcast across the underlying SMALL operation:
    // https://support.microsoft.com/en-us/excel/guidelines-and-examples-of-array-formulas
    let workbook = workbook();
    for (formula, expected) in [
        ("SUM(AGGREGATE(14,0,A1:A4,{1,2}))", 10.),
        ("INDEX(AGGREGATE(14,0,A1:A4,{1,2;0,4}),2,2)", 2.),
        ("ERROR.TYPE(INDEX(AGGREGATE(14,0,A1:A4,{1,2;0,4}),2,1))", 6.),
        ("SUM(IFERROR(AGGREGATE(14,0,A1:A4,{1,2;0,4}),0))", 12.),
        (
            "ERROR.TYPE(INDEX(AGGREGATE(14,0,A1:A4,{1,#DIV/0!}),1,2))",
            2.,
        ),
        ("SUM(AGGREGATE(15,0,A1:A4,SEQUENCE(2)))", 6.),
        ("COLUMNS(AGGREGATE(14,0,A1:A4,{1,2}))", 2.),
    ] {
        assert_eq!(
            value(&workbook, formula),
            CellValue::Number(expected),
            "{formula}"
        );
    }
    // The single-value convenience entry point projects the spill anchor.
    assert_eq!(
        value(&workbook, "AGGREGATE(14,0,A1:A4,{1,2})"),
        CellValue::Number(6.)
    );
    let mut spilled = workbook;
    spilled
        .sheet_mut(0)
        .unwrap()
        .set_formula("B1", "AGGREGATE(14,0,A1:A4,{1,2;0,4})")
        .unwrap();
    assert_eq!(value(&spilled, "C2"), CellValue::Number(2.));
    assert_eq!(value(&spilled, "ERROR.TYPE(B2)"), CellValue::Number(6.));
}
