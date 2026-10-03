use poirot_eval::{
    BudgetKind, CapabilityId, EvaluationBudgets, EvaluationFailure, EvaluationInputs,
    EvaluationOutcome, run_formula,
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
            match poirot_eval::run_formula(
                &w,
                0,
                AT,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Number(expected),
            "{formula}"
        );
    }
    assert_eq!(
        match poirot_eval::run_formula(
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
        CellValue::Error(CellError::Value)
    );
    assert_eq!(
        match poirot_eval::run_formula(
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
fn matrix_collector_compacts_ignored_values_then_pads_original_shape() {
    // MutableValueCollector drops area text/bools; fillDoubleArray pads zero.
    let w = workbook();
    for (formula, expected) in [
        (r#"INDEX(TRANSPOSE({1,"x";3,4}),1,2)"#, 4.),
        (r#"INDEX(TRANSPOSE({1,"x";3,4}),2,2)"#, 0.),
        (r#"TRANSPOSE("2")"#, 2.),
        ("TRANSPOSE(TRUE)", 1.),
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &w,
                0,
                AT,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Number(expected),
            "{formula}"
        );
    }
    assert_eq!(
        match poirot_eval::run_formula(
            &w,
            0,
            AT,
            "TRANSPOSE({TRUE})",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Value)
    );
}
#[test]
fn cell_references_and_one_cell_areas_keep_distinct_source_coercions() {
    let mut w = workbook();
    w.sheet_mut(0)
        .unwrap()
        .set_value(CellAddress { row: 0, col: 2 }, CellValue::Text("2".into()))
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value(CellAddress { row: 0, col: 3 }, CellValue::Boolean(true))
        .unwrap();
    for (formula, expected) in [
        ("MDETERM(A1)", CellValue::Number(1.)),
        ("MDETERM(A1:A1)", CellValue::Error(CellError::Value)),
        ("MMULT(A1,2)", CellValue::Number(2.)),
        ("MMULT(A1:A1,2)", CellValue::Error(CellError::Value)),
        ("TRANSPOSE(C1)", CellValue::Number(2.)),
        ("TRANSPOSE(C1:C1)", CellValue::Error(CellError::Value)),
        ("TRANSPOSE(D1)", CellValue::Number(1.)),
        ("TRANSPOSE(D1:D1)", CellValue::Error(CellError::Value)),
        // Frequency's collector ignores reference booleans, but counts direct ones.
        ("INDEX(FREQUENCY(D1,{1}),1,1)", CellValue::Number(0.)),
        ("INDEX(FREQUENCY(TRUE,{1}),1,1)", CellValue::Number(1.)),
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &w,
                0,
                AT,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{formula}"
        );
    }
}
#[test]
fn source_error_precedence_survives_shape_mismatch_and_scalar_dispatch() {
    let w = workbook();
    for (formula, expected) in [
        ("SUMPRODUCT({#N/A,2},{1,2,#DIV/0!})", CellError::Div0),
        ("SUMPRODUCT({#N/A,2},{1,2,3})", CellError::Value),
        ("SUMPRODUCT(#N/A)", CellError::Value),
        ("SUMPRODUCT({1,2},{1,#N/A})", CellError::NA),
        ("TREND({1;#N/A;3})", CellError::Value),
        ("MDETERM({1,#N/A;3,4})", CellError::NA),
        ("MDETERM(1,UNKNOWN_FUNCTION())", CellError::Value),
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &w,
                0,
                AT,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Error(expected),
            "{formula}"
        );
    }
}
#[test]
fn trend_retains_omitted_arguments_and_reports_source_malformed_area() {
    let w = workbook();
    let n = match poirot_eval::run_formula(
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
    assert!(matches!(
        run_formula(
            &w,
            0,
            AT,
            "IFERROR(TREND({1;3;4;6},{0,0;1,0;0,1;1,1},{2,3;4,5}),99)",
            EvaluationInputs::default(),
            EvaluationBudgets::default()
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::UnsupportedCapability {
                capability: CapabilityId::FunctionAdapter,
                ..
            },
            ..
        }
    ));
    // OperandResolver's MissingArgEval numeric coercion throws in the source;
    // preserve this capability gap instead of replacing syntax missing by blank.
    assert!(matches!(
        run_formula(
            &w,
            0,
            AT,
            "IFERROR(MMULT(,2),99)",
            EvaluationInputs::default(),
            EvaluationBudgets::default()
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::UnsupportedCapability {
                capability: CapabilityId::FunctionAdapter,
                ..
            },
            ..
        }
    ));
}
#[test]
fn operational_limits_abort_before_requested_reference_traversal() {
    let w = workbook();
    let outcome = run_formula(
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
        run_formula(
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
fn nested_scalar_consumers_use_source_specific_computed_area_coordinates() {
    // MatrixFunction.java:127,230 retain the input area's origin (array
    // literals originate at A1); Frequency.java:64 and Trend.java:89 use
    // the caller. OperandResolver.java:196-214 intersects those coordinates.
    let w = workbook();
    for (at, transpose, inverse, product) in [
        (CellAddress { row: 0, col: 0 }, 1., 2., 1.),
        (CellAddress { row: 1, col: 1 }, 4., 0.5, 4.),
    ] {
        for (formula, expected) in [
            ("ABS(TRANSPOSE(A1:B2))", transpose),
            ("ABS(MMULT(A1:B2,{1,0;0,1}))", product),
        ] {
            assert_eq!(
                match poirot_eval::run_formula(
                    &w,
                    0,
                    at,
                    formula,
                    Default::default(),
                    Default::default()
                ) {
                    poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                    outcome => panic!("evaluation aborted: {outcome:?}"),
                },
                CellValue::Number(expected),
                "{formula} at {at:?}"
            );
        }
        // Analytically inverse([1,2;3,4]) = [-2,1;1.5,-0.5].
        // A small rounding allowance checks this numerical result without
        // claiming bitwise parity between LU implementations.
        let formula = "ABS(MINVERSE({1,2;3,4}))";
        let actual = match poirot_eval::run_formula(
            &w,
            0,
            at,
            formula,
            Default::default(),
            Default::default(),
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        };
        let tolerance = 8. * f64::EPSILON * f64::max(inverse, 1.);
        assert!(
            matches!(actual, CellValue::Number(n) if (n - inverse).abs() <= tolerance),
            "{formula} at {at:?}: {actual:?}, expected {inverse}"
        );
    }
    for at in [
        CellAddress { row: 0, col: 0 },
        CellAddress { row: 1, col: 1 },
        CellAddress { row: 9, col: 9 },
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &w,
                0,
                at,
                "ABS(FREQUENCY({1,2,3},{2}))",
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Number(2.)
        );
        let value = match poirot_eval::run_formula(
            &w,
            0,
            at,
            "ABS(TREND({2;4;6},,{4;5}))",
            Default::default(),
            Default::default(),
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        };
        assert!(
            matches!(value, CellValue::Number(n) if (n - 8.).abs() <= 8. * f64::EPSILON * 8.),
            "{value:?} at {at:?}"
        );
    }
    for formula in [
        "ABS(TRANSPOSE(A1:B2))",
        "ABS(MINVERSE({1,2;3,4}))",
        "ABS(MMULT(A1:B2,{1,0;0,1}))",
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &w,
                0,
                CellAddress { row: 9, col: 9 },
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Error(CellError::Value),
            "{formula}"
        );
    }
}

#[test]
fn tall_and_wide_computed_areas_intersect_only_their_varying_axis() {
    let w = workbook();
    for (at, formula, expected) in [
        (
            CellAddress { row: 1, col: 9 },
            "ABS(TRANSPOSE({1,2,3}))",
            2.,
        ),
        (
            CellAddress { row: 9, col: 1 },
            "ABS(TRANSPOSE({1;2;3}))",
            2.,
        ),
        // MMULT chooses arg1 when arg0 is scalar, and arg0 when both are areas.
        (CellAddress { row: 9, col: 1 }, "ABS(MMULT(2,{3,4}))", 8.),
        (CellAddress { row: 1, col: 9 }, "ABS(MMULT({3;4},2))", 8.),
        (CellAddress { row: 9, col: 9 }, "ABS(MMULT(2,3))", 6.),
        (CellAddress { row: 9, col: 9 }, "ABS(TRANSPOSE(3))", 3.),
        (CellAddress { row: 9, col: 9 }, "ABS(MINVERSE(2))", 0.5),
        (
            CellAddress { row: 9, col: 9 },
            "INDEX(TRANSPOSE({1,2,3;4,5,6}),3,2)",
            6.,
        ),
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &w,
                0,
                at,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Number(expected),
            "{formula} at {at:?}"
        );
    }
    assert_eq!(
        match poirot_eval::run_formula(
            &w,
            0,
            AT,
            "INDEX(TRANSPOSE({1,2,3;4,5,6}),2,3)",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Ref)
    );
}

#[test]
fn matrix_output_preserves_non_a1_reference_origins_and_first_area_precedence() {
    let mut w = workbook();
    for (at, value) in [
        (CellAddress { row: 9, col: 9 }, 1.),
        (CellAddress { row: 9, col: 10 }, 0.),
        (CellAddress { row: 10, col: 9 }, 0.),
        (CellAddress { row: 10, col: 10 }, 1.),
    ] {
        w.sheet_mut(0).unwrap().set_value(at, value).unwrap();
    }
    for (at, formula, expected) in [
        (
            CellAddress { row: 9, col: 9 },
            "ABS(TRANSPOSE(J10:K11))",
            CellValue::Number(1.),
        ),
        (
            CellAddress { row: 10, col: 10 },
            "ABS(MINVERSE(J10:K11))",
            CellValue::Number(1.),
        ),
        (
            CellAddress { row: 0, col: 0 },
            "ABS(TRANSPOSE(J10:K11))",
            CellValue::Error(CellError::Value),
        ),
        (
            CellAddress { row: 9, col: 9 },
            "ABS(MMULT(2,J10:K10))",
            CellValue::Number(2.),
        ),
        (
            CellAddress { row: 1, col: 1 },
            "ABS(MMULT(A1:B2,J10:K11))",
            CellValue::Number(4.),
        ),
        (
            CellAddress { row: 9, col: 9 },
            "ABS(MMULT(A1:B2,J10:K11))",
            CellValue::Error(CellError::Value),
        ),
        // TRANSPOSE also inherits a nested Frequency CacheAreaEval's caller origin.
        (
            CellAddress { row: 9, col: 9 },
            "ABS(TRANSPOSE(FREQUENCY({1,2,3},{2})))",
            CellValue::Number(2.),
        ),
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &w,
                0,
                at,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{formula} at {at:?}"
        );
    }
}

#[test]
fn retained_poi_coordinate_observations_preserve_typed_results() {
    // Coordinate/error results are checked exactly; arithmetic is allowed eight
    // machine epsilons scaled by analytic magnitude because the Rust LU and
    // regression kernels need not match the Java implementation's final bits.
    let mut w = workbook();
    w.add_sheet("Data").unwrap();
    let data = 1;
    for (row, values) in [[1., 2.], [3., 4.]].iter().enumerate() {
        for (col, value) in values.iter().enumerate() {
            w.sheet_mut(data)
                .unwrap()
                .set_value(
                    CellAddress {
                        row: 9 + row as u32,
                        col: 9 + col as u32,
                    },
                    *value,
                )
                .unwrap();
        }
    }
    let callers = [
        CellAddress { row: 0, col: 0 },
        CellAddress { row: 1, col: 1 },
        CellAddress { row: 9, col: 9 },
    ];
    for (formula, results) in [
        (
            "ABS(TRANSPOSE({1,2,3}))",
            [
                CellValue::Number(1.),
                CellValue::Number(2.),
                CellValue::Error(CellError::Value),
            ],
        ),
        (
            "ABS(TRANSPOSE(Data!J10:K11))",
            [
                CellValue::Error(CellError::Value),
                CellValue::Error(CellError::Value),
                CellValue::Number(1.),
            ],
        ),
        (
            "ABS(MMULT(Data!J10:K11,{1,0;0,1}))",
            [
                CellValue::Error(CellError::Value),
                CellValue::Error(CellError::Value),
                CellValue::Number(1.),
            ],
        ),
        (
            "ABS(FREQUENCY({1,2,3},{2}))",
            [
                CellValue::Number(2.),
                CellValue::Number(2.),
                CellValue::Number(2.),
            ],
        ),
    ] {
        for (at, expected) in callers.into_iter().zip(results) {
            assert_eq!(
                match poirot_eval::run_formula(
                    &w,
                    0,
                    at,
                    formula,
                    Default::default(),
                    Default::default()
                ) {
                    poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                    outcome => panic!("evaluation aborted: {outcome:?}"),
                },
                expected,
                "{formula} at {at:?}"
            );
        }
    }
    for (at, analytic, native) in [
        (callers[0], 2_f64, 1.9999999999999998_f64),
        (callers[1], 0.5_f64, 0.49999999999999994_f64),
    ] {
        let actual = match poirot_eval::run_formula(
            &w,
            0,
            at,
            "ABS(MINVERSE({1,2;3,4}))",
            Default::default(),
            Default::default(),
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        };
        let tolerance = 8. * f64::EPSILON * analytic.max(1.);
        assert!(
            matches!(actual, CellValue::Number(n) if (n - analytic).abs() <= tolerance),
            "MINVERSE at {at:?}: {actual:?}, analytic {analytic}, native {native}"
        );
    }
    assert_eq!(
        match poirot_eval::run_formula(
            &w,
            0,
            callers[2],
            "ABS(MINVERSE({1,2;3,4}))",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Value)
    );
    for at in callers {
        let actual = match poirot_eval::run_formula(
            &w,
            0,
            at,
            "ABS(TREND({2;4;6},,{4;5}))",
            Default::default(),
            Default::default(),
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        };
        let tolerance = 8. * f64::EPSILON * 8.;
        assert!(
            matches!(actual, CellValue::Number(n) if (n - 8.).abs() <= tolerance),
            "TREND at {at:?}: {actual:?}, analytic 8, native 7.999999999999999"
        );
    }
}
