use poirot_eval::{
    BudgetKind, EvaluationBudgets, EvaluationFailure, EvaluationInputs, EvaluationOutcome,
};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};

const AT: CellAddress = CellAddress { row: 8, col: 8 };
fn workbook() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    for (row, values) in [[1., 2.], [3., 4.]].iter().enumerate() {
        for (col, value) in values.iter().enumerate() {
            w.sheet_mut(0)
                .unwrap()
                .set_value(
                    CellAddress {
                        row: row as u32,
                        col: col as u32,
                    },
                    *value,
                )
                .unwrap();
        }
    }
    w
}
#[test]
fn genuine_arrays_and_reference_areas_reach_matrix_helpers() {
    let w = workbook();
    for (formula, expected) in [
        ("MDETERM({1,2;3,4})", -2.),
        ("MDETERM(A1:B2)", -2.),
        ("MDETERM(A1)", 1.),
        ("INDEX(TRANSPOSE(A1:B2),1,2)", 3.),
        ("INDEX(MMULT({1,2;3,4},{1,0;0,1}),2,1)", 3.),
        ("SUMPRODUCT(A1:B2,{2,2;2,2})", 20.),
        ("SUMPRODUCT({TRUE,FALSE},{2,9})", 2.),
        ("INDEX(FREQUENCY({1,2,3,4},{2,3}),3,1)", 1.),
    ] {
        assert_eq!(
            match modern_formula(&w, 0, AT, formula, Default::default(), Default::default()) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Number(expected),
            "{formula}"
        );
    }
    assert_eq!(
        match modern_formula(
            &w,
            0,
            AT,
            "MDETERM({1})",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(1.0)
    );
    assert_eq!(
        match modern_formula(
            &w,
            0,
            AT,
            "MINVERSE({1,2;2,4})",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Num)
    );
}
#[test]
fn documented_shape_errors_and_numeric_member_errors_remain_explicit() {
    let w = workbook();
    for (formula, expected) in [
        ("SUMPRODUCT({#N/A,2},{1,2,#DIV/0!})", CellError::Value),
        ("SUMPRODUCT({#N/A,2},{1,2,3})", CellError::Value),
        ("SUMPRODUCT(#N/A)", CellError::NA),
        ("SUMPRODUCT({1,2},{1,#N/A})", CellError::NA),
        ("TREND({1;#N/A;3})", CellError::NA),
        ("MDETERM({1,#N/A;3,4})", CellError::NA),
        ("MDETERM(1,UNKNOWN_FUNCTION())", CellError::Value),
    ] {
        assert_eq!(
            match modern_formula(&w, 0, AT, formula, Default::default(), Default::default()) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Error(expected),
            "{formula}"
        );
    }
}
#[test]
fn trend_retains_omitted_arguments_and_rectangular_predictors() {
    let w = workbook();
    let n = match modern_formula(
        &w,
        0,
        AT,
        "INDEX(TREND({2;4;6},,{4;5}),2,1)",
        Default::default(),
        Default::default(),
    ) {
        poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("evaluation aborted: {outcome:?}"),
    };
    assert!(matches!(n, CellValue::Number(n) if (n-10.).abs()<1e-9));
    // Independently constructed affine response y=1+2*x1+3*x2 predicts 14 and 24.
    let result = modern_formula(
        &w,
        0,
        AT,
        "INDEX(TREND({1;3;4;6},{0,0;1,0;0,1;1,1},{2,3;4,5}),2,1)",
        Default::default(),
        Default::default(),
    );
    assert!(
        matches!(result, EvaluationOutcome::Complete { value: CellValue::Number(n), .. } if (n-24.).abs()<1e-9)
    );
}
#[test]
fn operational_limits_abort_before_requested_reference_traversal() {
    let w = workbook();
    let outcome = modern_formula(
        &w,
        0,
        AT,
        "IFERROR(MINVERSE(A1:B2),99)",
        EvaluationInputs::default(),
        EvaluationBudgets {
            materialized_values: 3,
            ..EvaluationBudgets::default()
        },
    );
    match outcome {
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(failure),
            usage,
        } => {
            assert_eq!(failure.kind, BudgetKind::Materialization);
            assert_eq!(failure.requested, 4);
            assert_eq!(usage.cell_reads, 0);
        }
        other => panic!("resource exhaustion must remain operational: {other:?}"),
    }
    assert!(matches!(
        modern_formula(
            &w,
            0,
            AT,
            "IFERROR(MINVERSE({1,0;0,1}),99)",
            EvaluationInputs::default(),
            EvaluationBudgets {
                run_work: 10,
                ..EvaluationBudgets::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
}

#[test]
fn documented_numeric_matrix_admission_preserves_types() {
    use poirot_eval::{BuiltinSemanticsPolicy, RunConfiguration, run_formula_with_configuration};
    let mut w = workbook();
    for invalid in [
        CellValue::Blank,
        CellValue::Text("2".into()),
        CellValue::Text("x".into()),
        CellValue::Boolean(true),
        CellValue::Boolean(false),
    ] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(CellAddress { row: 0, col: 1 }, invalid)
            .unwrap();
        for f in ["MDETERM(A1:B2)", "MINVERSE(A1:B2)"] {
            let c = RunConfiguration::default()
                .with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation);
            assert!(
                matches!(
                    run_formula_with_configuration(&w, 0, AT, f, &c, EvaluationBudgets::default()),
                    EvaluationOutcome::Complete {
                        value: CellValue::Error(CellError::Value),
                        ..
                    }
                ),
                "{f}"
            );
        }
    }
    for (formula, expected) in [
        (
            r#"MDETERM({1,"2";3,4})"#,
            CellValue::Error(CellError::Value),
        ),
        (
            r#"MINVERSE({1,TRUE;3,4})"#,
            CellValue::Error(CellError::Value),
        ),
        ("MDETERM({0,1;1,0})", CellValue::Number(-1.)),
        ("MDETERM({2})", CellValue::Number(2.)),
        ("MINVERSE({2})", CellValue::Number(0.5)),
        ("MDETERM({1,2,3})", CellValue::Error(CellError::Value)),
        ("MINVERSE({1,#N/A})", CellValue::Error(CellError::Value)),
        ("MINVERSE({1,2;2,4})", CellValue::Error(CellError::Num)),
        (
            "MDETERM({1,#N/A;3,#DIV/0!})",
            CellValue::Error(CellError::NA),
        ),
        (
            r#"MDETERM({1,#N/A;3,"x"})"#,
            CellValue::Error(CellError::Value),
        ),
    ] {
        let c = RunConfiguration::default()
            .with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation);
        match run_formula_with_configuration(&w, 0, AT, formula, &c, EvaluationBudgets::default()) {
            EvaluationOutcome::Complete { value, .. } => assert_eq!(value, expected, "{formula}"),
            other => panic!("{formula}: {other:?}"),
        }
    }
    let c =
        RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation);
    for budgets in [
        EvaluationBudgets {
            materialized_values: 3,
            ..EvaluationBudgets::default()
        },
        EvaluationBudgets {
            run_work: 10,
            ..EvaluationBudgets::default()
        },
    ] {
        assert!(matches!(
            run_formula_with_configuration(&w, 0, AT, "IFERROR(MINVERSE(A1:B2),99)", &c, budgets),
            EvaluationOutcome::Aborted {
                failure: EvaluationFailure::Budget(_),
                ..
            }
        ));
    }
}

/// Execute documented matrix controls under modern semantics.
fn modern_formula(
    workbook: &Workbook,
    sheet: usize,
    address: CellAddress,
    formula: &str,
    inputs: EvaluationInputs,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    poirot_eval::run_formula_with_configuration(
        workbook,
        sheet,
        address,
        formula,
        &poirot_eval::RunConfiguration::from(inputs)
            .with_builtin_policy(poirot_eval::BuiltinSemanticsPolicy::ExcelDocumentation)
            .with_array_result_policy(poirot_eval::ArrayResultPolicy::ScalarIntersection),
        budgets,
    )
}
