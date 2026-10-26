//! Independent public-boundary probes for selected helper MissingArgEval identity.
use poirot_eval::{
    EvaluationBudgets, EvaluationFailure, EvaluationInputs, EvaluationOutcome, run_formula,
};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};

fn execute(formula: &str) -> EvaluationOutcome<CellValue> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    run_formula(
        &workbook,
        0,
        CellAddress { row: 2, col: 2 },
        formula,
        EvaluationInputs::default(),
        EvaluationBudgets::default(),
    )
}

#[test]
fn selected_missing_retains_source_exception_boundaries() {
    // A selected omitted result still lacks worksheet publication support.
    // Preserve this known operational gap rather than claiming native Excel behavior.
    for formula in ["IFS(TRUE,)", "IFERROR(IFS(TRUE,),42)"] {
        assert!(
            matches!(
                execute(formula),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::UnsupportedCapability { .. },
                    ..
                }
            ),
            "{formula}"
        );
    }
}

#[test]
fn blank_cells_lazy_branches_and_error_type_keep_distinct_semantics() {
    for (formula, expected) in [
        ("IFS(TRUE,A1)", CellValue::Blank),
        ("ISEVEN(A1)", CellValue::Boolean(true)),
        ("ERROR.TYPE(IFS(TRUE,))", CellValue::Error(CellError::NA)),
        ("IFERROR(ERROR.TYPE(IFS(TRUE,)),42)", CellValue::Number(42.)),
        ("IFS(TRUE,7,FALSE,)", CellValue::Number(7.)),
        ("IF(FALSE,ISEVEN(IFS(TRUE,)),7)", CellValue::Number(7.)),
    ] {
        match execute(formula) {
            EvaluationOutcome::Complete { value, .. } => assert_eq!(value, expected, "{formula}"),
            other => panic!("{formula}: {other:?}"),
        }
    }
}

#[test]
fn final_missing_bulk_abort_publishes_no_partial_cache_or_style_changes() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    for (address, formula, cache) in [("A1", "1+1", 101.), ("A2", "IFS(TRUE,)", 102.)] {
        let address = address.parse().unwrap();
        let sheet = workbook.sheet_mut(0).unwrap();
        sheet
            .set_formula_with_cached_value(address, formula, CellValue::Number(cache))
            .unwrap();
        sheet.get_mut(address).unwrap().style.number_format = Some("0.00".into());
    }
    let before = workbook.clone();
    match poirot_eval::run_recalculation(
        &mut workbook,
        EvaluationInputs::default(),
        EvaluationBudgets::default(),
    ) {
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::UnsupportedCapability { .. },
            usage,
        } => {
            assert!(usage.completed_cells >= 1);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(workbook, before);
}
