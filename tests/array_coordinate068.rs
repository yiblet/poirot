//! Independent coordinate probes observed from pinned POI HSSF FormulaEvaluator.
use poirot::model::{CellAddress, CellError, CellValue, Workbook};

#[test]
fn gcd_literal_array_uses_caller_coordinates_for_source_scalar_selection() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    for (address, expected) in [
        ("A1", CellValue::Number(2.)),
        ("B1", CellValue::Number(4.)),
        ("J10", CellValue::Error(CellError::Value)),
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &workbook,
                0,
                address.parse::<CellAddress>().unwrap(),
                "GCD({2,4})",
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{address}"
        );
    }
}

#[test]
fn scalar_selection_preserves_nested_and_adjusted_literal_array_anchors() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    for (address, formula, expected) in [
        ("B10", "GCD({2,4})", CellValue::Number(4.)),
        ("A2", "GCD({2;4})", CellValue::Number(4.)),
        ("J2", "GCD({2;4})", CellValue::Number(4.)),
        ("B2", "GCD({2,4;6,8})", CellValue::Number(8.)),
        ("C3", "GCD({2,4;6,8})", CellValue::Error(CellError::Value)),
        ("B1", "GCD(IFS(TRUE,{2,4}))", CellValue::Number(4.)),
        ("J10", "GCD(INDEX({2,4},1,2))", CellValue::Number(4.)),
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &workbook,
                0,
                address.parse().unwrap(),
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{address}: {formula}"
        );
    }
}

#[test]
fn source_unsupported_topologies_do_not_become_catchable_errors() {
    use poirot::eval::{
        EvaluationBudgets, EvaluationFailure, EvaluationInputs, EvaluationOutcome, run_formula,
    };
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    for (address, value) in [("A1", 2.), ("B1", 4.), ("A2", 6.), ("B2", 8.)] {
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value(address.parse().unwrap(), CellValue::Number(value))
            .unwrap();
    }
    assert_eq!(
        match poirot_eval::run_formula(
            &workbook,
            0,
            "B2".parse().unwrap(),
            "GCD(A1:B2)",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(8.)
    );
    // MMULT retains the first area's A1 origin; GCD intersects B2.
    assert_eq!(
        match poirot_eval::run_formula(
            &workbook,
            0,
            "B2".parse().unwrap(),
            "IFERROR(GCD(MMULT({1,0;0,1},{2,4;6,8})),42)",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(8.)
    );
    {
        let formula = "IFERROR(GCD((A1,B1)),42)";
        assert!(
            matches!(
                run_formula(
                    &workbook,
                    0,
                    "B2".parse().unwrap(),
                    formula,
                    EvaluationInputs::default(),
                    EvaluationBudgets::default()
                ),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::UnsupportedCapability { .. },
                    ..
                }
            ),
            "{formula}"
        );
    }
}
