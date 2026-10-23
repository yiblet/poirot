//! INDEX contracts from Microsoft INDEX documentation and pinned ClosedXML LookupTests.
//! Calc corroboration is recorded in evidence/index-admission-libreoffice.json.
use poirot_eval::{
    overlay::{EvaluationContext, Overlay, OverlayPolicy},
    *,
};
use poirot_model::{CellAddress, CellError, CellValue, ExcelText, Workbook};

fn target(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
fn book() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("S").unwrap();
    for (address, value) in [("A1", 1.), ("B1", 2.), ("A2", 3.), ("B2", 4.), ("C1", 9.)] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(address.parse().unwrap(), value)
            .unwrap();
    }
    w.sheet_mut(0)
        .unwrap()
        .set_value(
            "D1".parse().unwrap(),
            CellValue::TextUtf16(ExcelText::from_units(vec![65, 0xd800])),
        )
        .unwrap();
    w
}
fn config() -> RunConfiguration {
    RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation)
}
fn value(w: &Workbook, formula: &str, config: &RunConfiguration) -> CellValue {
    match run_formula_with_configuration(
        w,
        0,
        CellAddress { row: 0, col: 0 },
        formula,
        config,
        EvaluationBudgets::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{formula}: {other:?}"),
    }
}
#[test]
fn index_validates_area_bounds_without_inventing_references() {
    use CellError::{Ref, Value};
    use CellValue::{Error, Number};
    let w = book();
    for source in [
        "1",
        "{1,2;3,4}",
        "TRANSPOSE({1,2;3,4})",
        "A1:B2",
        "(A1:B2,C1:D2)",
    ] {
        for (area, expected) in [
            ("0", Error(Value)),
            ("-1", Error(Value)),
            ("0.9", Error(Value)),
            ("1", Number(1.)),
            ("1.9", Number(1.)),
            ("3", Error(Ref)),
            ("1/0", Error(CellError::Div0)),
        ] {
            let formula = format!("INDEX({source},1,1,{area})");
            assert_eq!(value(&w, &formula, &config()), expected, "{formula}");
        }
        for (row, col, expected) in [
            ("-1", "1", Error(Value)),
            ("-0.1", "1", Error(Value)),
            ("1", "-1", Error(Value)),
            ("3", "1", Error(Ref)),
            ("1", "3", Error(Ref)),
            ("1/0", "SQRT(-1)", Error(CellError::Num)),
        ] {
            let formula = format!("INDEX({source},{row},{col},1)");
            assert_eq!(value(&w, &formula, &config()), expected, "{formula}");
        }
    }
    for (formula, expected) in [
        ("INDEX((A1:B2,C1:D2),1,1,2)", Number(9.)),
        ("INDEX((A1:B2,C1:D2),1,1)", Number(1.)),
        ("INDEX((A1:B2,C1:D2),1,1,)", Number(1.)),
        ("INDEX(A1,1,1,E1)", Error(Value)),
        ("INDEX({1,2;3,4},2.9,1)", Number(3.)),
        ("SUM(INDEX({1,2;3,4},,2))", Number(6.)),
        ("SUM(INDEX({1,2;3,4},2,))", Number(7.)),
        ("SUM(INDEX(TRANSPOSE({1,2;3,4}),0,1))", Number(3.)),
        ("SUM(INDEX(A1:B2,0,0))", Number(10.)),
        ("INDEX(1/0,-1,SQRT(-1),0)", Error(CellError::Div0)),
    ] {
        assert_eq!(value(&w, formula, &config()), expected, "{formula}");
    }
}
#[test]
fn isref_classifies_selected_representation_under_the_requested_contract() {
    let w = book();
    for source in [
        "{1,2;3,4}",
        "TRANSPOSE({1,2;3,4})",
        "INDEX({1,2;3,4},0,1)",
        "INDEX({1,2;3,4},1,1)",
        "IF(TRUE,{1,2},0)",
        "CHOOSE(1,{1,2},0)",
    ] {
        let formula = format!("ISREF({source})");
        assert_eq!(
            value(&w, &formula, &config()),
            CellValue::Boolean(false),
            "{formula}"
        );
        assert_eq!(
            value(
                &w,
                &formula,
                &RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi)
            ),
            CellValue::Boolean(true),
            "pinned {formula}"
        );
    }
    for source in [
        "A1:B2",
        "(A1,B1)",
        "INDEX(A1:B2,0,1)",
        "INDEX((A1,B1),1,1,2)",
        "IF(TRUE,A1:B2,0)",
        "CHOOSE(1,A1:B2,0)",
        "INDIRECT(\"A1:B2\")",
    ] {
        assert_eq!(
            value(&w, &format!("ISREF({source})"), &config()),
            CellValue::Boolean(true),
            "{source}"
        );
    }
    for source in ["1", "INDEX(1,1,1)", "INDEX({1},1,1)"] {
        for consumer in ["ROWS", "COLUMNS"] {
            assert_eq!(
                value(&w, &format!("{consumer}({source})"), &config()),
                CellValue::Number(1.)
            );
        }
    }
    assert_eq!(
        value(&w, "ROWS(INDEX({1,2;3,4},0,2))", &config()),
        CellValue::Number(2.)
    );
    assert_eq!(
        value(&w, "COLUMNS(INDEX({1,2;3,4},2,0))", &config()),
        CellValue::Number(2.)
    );
}
#[test]
fn index_preserves_blank_types_utf16_and_deferred_source_reads() {
    let w = book();
    for formula in ["INDEX(E1,1,1)", "INDEX(INDEX(E1,1,1),1,1)"] {
        assert_eq!(value(&w, formula, &config()), CellValue::Blank);
        assert_eq!(
            value(&w, &format!("ISBLANK({formula})"), &config()),
            CellValue::Boolean(true)
        );
    }
    for formula in ["INDEX(D1,1,1)", "INDEX(INDEX(D1,1,1),1,1)"] {
        assert_eq!(
            value(&w, formula, &config())
                .text_ref()
                .unwrap()
                .units()
                .collect::<Vec<_>>(),
            vec![65, 0xd800]
        );
    }
    assert_eq!(
        value(&w, "ISTEXT(INDEX({\"12\",TRUE},1,1))", &config()),
        CellValue::Boolean(true)
    );
    assert_eq!(
        value(&w, "ISLOGICAL(INDEX({\"12\",TRUE},1,2))", &config()),
        CellValue::Boolean(true)
    );
    for formula in [
        "IFERROR(INDEX(A1:B2,1/0,1,0),7)",
        "IFERROR(INDEX({1},1,1,2),7)",
    ] {
        assert_eq!(value(&w, formula, &config()), CellValue::Number(7.));
    }
}
#[test]
fn selected_index_results_refresh_after_source_edits_and_policy_switches() {
    let mut w = book();
    for (address, formula) in [
        ("F1", "SUM(INDEX(A1:B2,0,2))"),
        ("G1", "ISREF(INDEX({1,2},0,1))"),
        ("H1", "F1+1"),
    ] {
        w.sheet_mut(0)
            .unwrap()
            .set_formula(address.parse().unwrap(), formula)
            .unwrap();
    }
    let mut overlay = Overlay::with_workbook(&w)
        .with_policy(poirot_eval::overlay::OverlayPolicy {
            builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    for _ in 0..2 {
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("H1"))
                .unwrap(),
            CellValue::Number(7.)
        );
    }
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("G1"))
            .unwrap(),
        CellValue::Boolean(false)
    );
    overlay.set_value(target("B2"), 10.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("H1"))
            .unwrap(),
        CellValue::Number(13.)
    );
    overlay
        .set_policy(OverlayPolicy {
            builtin: poirot_eval::BuiltinSemanticsPolicy::PinnedPoi,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("G1"))
            .unwrap(),
        CellValue::Boolean(true)
    );
}
#[test]
fn index_resource_failures_escape_semantic_error_handlers() {
    let w = book();
    for formula in [
        "IFERROR(INDEX(TRANSPOSE({1,2;3,4}),1,1,0),7)",
        "IFERROR(INDEX(A1:B2,1,1,TRANSPOSE({1,2;3,4})),7)",
    ] {
        assert!(
            matches!(
                run_formula_with_configuration(
                    &w,
                    0,
                    target("Z100").address,
                    formula,
                    &config(),
                    EvaluationBudgets {
                        materialized_values: 3,
                        ..Default::default()
                    }
                ),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::Budget(_),
                    ..
                }
            ),
            "{formula}"
        );
    }
}

#[test]
fn index_array_slices_publish_actual_dynamic_footprints() {
    let mut w = book();
    for (address, formula) in [
        ("F1", "INDEX({1,2;3,4},0,2)"),
        ("H1", "INDEX(TRANSPOSE({1,2;3,4}),2,0)"),
        ("J1", "INDEX(A1:B2,0,1)"),
    ] {
        w.sheet_mut(0)
            .unwrap()
            .set_formula(target(address).address, formula)
            .unwrap();
    }
    let policy = EvaluationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&w, &policy);
    let config = config().with_array_result_policy(ArrayResultPolicy::DynamicSpill);
    let mut run = EvaluationRun::new_with_configuration(
        WorkbookView::new(&w, &plan, &policy).unwrap(),
        &config,
        EvaluationBudgets::default(),
    )
    .unwrap();
    for (address, expected) in [("F1", 2.), ("F2", 4.), ("H1", 2.), ("I1", 4.)] {
        assert_eq!(
            run.evaluate_cell(target(address)).unwrap(),
            CellValue::Number(expected),
            "{address}"
        );
    }
    assert_eq!(
        run.evaluate_cell(target("J1")).unwrap(),
        CellValue::Number(1.)
    );
    // Microsoft documents row zero as the entire selected column; dynamic root
    // materialization now preserves the reference channel instead of scalarizing it.
    // https://support.microsoft.com/en-us/excel/functions/index-function
    assert_eq!(
        run.evaluate_cell(target("J2")).unwrap(),
        CellValue::Number(3.)
    );
}
