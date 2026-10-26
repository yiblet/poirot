//! Public unary math contracts, independently derived from Microsoft documentation.
//! Array guide: <https://support.microsoft.com/en-us/excel/guidelines-and-examples-of-array-formulas>
//! (explicit rectangular SQRT example), dynamic-array and @ contracts:
//! <https://support.microsoft.com/en-us/excel/dynamic-array-formulas-and-spilled-array-behavior>
//! <https://support.microsoft.com/en-us/excel/implicit-intersection-operator>
//! Domains: <https://support.microsoft.com/en-us/excel/functions/sqrt-function>,
//! <https://support.microsoft.com/en-us/excel/functions/acosh-function> and
//! <https://support.microsoft.com/en-us/excel/functions/atanh-function>.
//! Retrieved 2026-10-25. No upstream code, tests, or data reused.
use poirot::eval::{
    ArrayResultPolicy, EvaluationBudgets, EvaluationFailure, EvaluationOutcome, RunConfiguration,
    run_formula_with_configuration,
};
use poirot::model::{CellValue, Workbook};
use poirot::{CellError, EvaluationContext, EvaluationTarget};

/// Evaluate a formula at a fixed row using the public configuration boundary.
fn run(
    book: &Workbook,
    formula: &str,
    configuration: &RunConfiguration,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        book,
        0,
        "J2".parse().unwrap(),
        formula,
        configuration,
        budgets,
    )
}

/// Extract a completed formula value while exposing unexpected operational aborts.
fn value(outcome: EvaluationOutcome<CellValue>) -> CellValue {
    match outcome {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("unexpected evaluation outcome: {other:?}"),
    }
}

/// Build a rectangular reference whose square roots are small distinct integers.
fn book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    for (address, number) in [("A1", 1.), ("B1", 4.), ("A2", 9.), ("B2", 16.)] {
        sheet
            .set_value(address.parse().unwrap(), CellValue::Number(number))
            .unwrap();
    }
    book
}

#[test]
fn conventional_unary_functions_lift_every_rectangular_member() {
    let book = book();
    // Mathematical identities provide independent expected results. The array
    // guide's SQRT example and general dynamic-array contract supply the lifting.
    for (formula, expected) in [
        ("SUM(ACOSH({1,1;1,1}))", 0.),
        ("SUM(ASINH({0,0;0,0}))", 0.),
        ("SUM(ATANH({0,0;0,0}))", 0.),
        ("SUM(COSH({0,0;0,0}))", 4.),
        ("SUM(SINH({0,0;0,0}))", 0.),
        ("SUM(TANH({0,0;0,0}))", 0.),
        ("SUM(SQRTPI({0,0;0,0}))", 0.),
        ("SUM(SQRT({1,4;9,16}))", 10.),
        ("SUM(SQRT(A1:B2))", 10.),
        ("SUM(EXP({0,0;0,0}))", 4.),
        ("SUM(LN({1,1;1,1}))", 0.),
        ("SUM(LOG10({1,10;100,1000}))", 6.),
        ("SUM(INT({1.9,-1.1;3.2,-3.2}))", -2.),
        ("SUM(SIN({0,0;0,0}))", 0.),
        ("SUM(COS({0,0;0,0}))", 4.),
        ("SUM(TAN({0,0;0,0}))", 0.),
        ("SUM(ASIN({0,0;0,0}))", 0.),
        ("SUM(ACOS({1,1;1,1}))", 0.),
        ("SUM(ATAN({0,0;0,0}))", 0.),
        ("SUM(DEGREES({0,0;0,0}))", 0.),
        ("SUM(RADIANS({0,0;0,0}))", 0.),
        ("SUM(SQRT(ABS({-1,-4;-9,-16})))", 10.),
        ("SUM(INT(SQRT(A1:B2)))", 10.),
        ("SUM(DEGREES(ASIN({0,1})))", 90.),
        ("SUM(SIN(RADIANS({0,90})))", 1.),
        ("SQRT({49})", 7.),
    ] {
        let actual = value(run(&book, formula, &Default::default(), Default::default()));
        let CellValue::Number(actual) = actual else {
            panic!("{formula}: {actual:?}");
        };
        assert!((actual - expected).abs() < 1e-12, "{formula}: {actual}");
    }
    for function in [
        "SQRT", "EXP", "LN", "LOG10", "INT", "SIN", "COS", "TAN", "ASIN", "ACOS", "ATAN", "EVEN",
        "ODD", "DEGREES", "RADIANS", "ACOSH", "ASINH", "ATANH", "COSH", "SINH", "TANH", "SQRTPI",
    ] {
        for (selector, expected) in [("ROWS", 2.), ("COLUMNS", 2.)] {
            let formula = format!("{selector}({function}({{1,1;1,1}}))");
            assert_eq!(
                value(run(
                    &book,
                    &formula,
                    &Default::default(),
                    Default::default()
                )),
                CellValue::Number(expected),
                "{formula}"
            );
        }
    }
}

#[test]
fn domains_and_input_errors_remain_local_and_nonfinite_outputs_become_num() {
    let book = book();
    for (formula, expected) in [
        ("INDEX(SQRT({4,-1;9,16}),1,1)", CellValue::Number(2.)),
        (
            "INDEX(SQRT({4,-1;9,16}),1,2)",
            CellValue::Error(CellError::Num),
        ),
        ("INDEX(SQRT({4,-1;9,16}),2,2)", CellValue::Number(4.)),
        ("INDEX(SQRT({4,#N/A}),1,2)", CellValue::Error(CellError::NA)),
        (
            "INDEX(SQRT({4,\"bad\"}),1,2)",
            CellValue::Error(CellError::Value),
        ),
        ("INDEX(ACOSH({1,0}),1,2)", CellValue::Error(CellError::Num)),
        ("INDEX(ATANH({0,1}),1,2)", CellValue::Error(CellError::Num)),
        (
            "INDEX(SQRTPI({0,-1}),1,2)",
            CellValue::Error(CellError::Num),
        ),
        (
            "INDEX(COSH({0,1000}),1,2)",
            CellValue::Error(CellError::Num),
        ),
        ("INDEX(LN({1,0}),1,2)", CellValue::Error(CellError::Num)),
        (
            "INDEX(LOG10({10,-1}),1,2)",
            CellValue::Error(CellError::Num),
        ),
        ("INDEX(ASIN({0,2}),1,2)", CellValue::Error(CellError::Num)),
        ("INDEX(ACOS({1,-2}),1,2)", CellValue::Error(CellError::Num)),
        ("INDEX(EXP({0,1000}),1,2)", CellValue::Error(CellError::Num)),
        (
            "INDEX(DEGREES({0,1e308}),1,2)",
            CellValue::Error(CellError::Num),
        ),
        (
            "IFERROR(INDEX(SQRT({4,-1;9,16}),1,2),99)",
            CellValue::Number(99.),
        ),
        ("SQRT()", CellValue::Error(CellError::Value)),
        (
            "SQRT(1,UNKNOWN_FUNCTION())",
            CellValue::Error(CellError::Value),
        ),
    ] {
        assert_eq!(
            value(run(&book, formula, &Default::default(), Default::default())),
            expected,
            "{formula}"
        );
    }
}

#[test]
fn explicit_intersection_and_final_projection_stay_distinct() {
    let book = book();
    let scalar =
        RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::ScalarIntersection);
    for (configuration, formula, expected) in [
        (
            &RunConfiguration::default(),
            "SQRT(@A1:A2)",
            CellValue::Number(3.),
        ),
        (
            &RunConfiguration::default(),
            "@SQRT(A1:A2)",
            CellValue::Number(1.),
        ),
        (
            &RunConfiguration::default(),
            "SQRT(@{4;9})",
            CellValue::Number(2.),
        ),
        (&scalar, "SQRT(A1:A2)", CellValue::Number(1.)),
        (&scalar, "SUM(SQRT(A1:A2))", CellValue::Number(4.)),
        (&scalar, "SUM(SQRT({1,4;9,16}))", CellValue::Number(10.)),
        (&scalar, "SUM(COSH({0;0}))", CellValue::Number(2.)),
    ] {
        assert_eq!(
            value(run(&book, formula, configuration, Default::default())),
            expected,
            "{formula} ({configuration:?})"
        );
    }
}

#[test]
fn reference_output_spills_in_direct_and_overlay_evaluations() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("D1".parse().unwrap(), "SQRT(A1:B2)")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("F1".parse().unwrap(), "COSH({0,0;0,0})")
        .unwrap();
    for (address, formula) in [("H1", "EVEN(A1:B2)"), ("K1", "ODD(A1:B2)")] {
        book.sheet_mut(0)
            .unwrap()
            .set_formula(address.parse().unwrap(), formula)
            .unwrap();
    }
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    let mut overlay = poirot::eval::overlay::Overlay::with_workbook(&book)
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let mut incremental = overlay.evaluation(&context).unwrap();
    // Child-first access also verifies spill-candidate inference for this family.
    for (address, expected) in [
        ("E2", 4.),
        ("D2", 3.),
        ("E1", 2.),
        ("D1", 1.),
        ("I2", 16.),
        ("H1", 2.),
        ("L2", 17.),
        ("K1", 1.),
        ("G2", 1.),
        ("F1", 1.),
    ] {
        let target = EvaluationTarget {
            sheet: 0,
            address: address.parse().unwrap(),
        };
        assert_eq!(direct.cell(target).unwrap(), CellValue::Number(expected));
        assert_eq!(
            incremental.cell(target).unwrap(),
            CellValue::Number(expected)
        );
    }
}

#[test]
fn broadcast_resource_failures_cannot_be_caught_as_spreadsheet_errors() {
    let book = book();
    let formula = "IFERROR(SUM(SQRT(A1:B2)),99)";
    let baseline = run(&book, formula, &Default::default(), Default::default());
    let work = baseline.usage().work_units;
    assert_eq!(value(baseline), CellValue::Number(10.));
    assert_eq!(
        value(run(
            &book,
            formula,
            &Default::default(),
            EvaluationBudgets {
                run_work: work,
                ..Default::default()
            }
        )),
        CellValue::Number(10.)
    );
    for budgets in [
        EvaluationBudgets {
            run_work: work - 1,
            ..Default::default()
        },
        EvaluationBudgets {
            materialized_values: 2,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            run(&book, formula, &Default::default(), budgets),
            EvaluationOutcome::Aborted {
                failure: EvaluationFailure::Budget(_),
                ..
            }
        ));
    }
}

#[test]
fn parity_rounding_broadcasts_away_from_zero_and_preserves_scalar_controls() {
    // Independent controls from Microsoft's EVEN/ODD contracts (2026-10-25):
    // https://support.microsoft.com/en-us/excel/functions/even-function
    // https://support.microsoft.com/en-us/excel/functions/odd-function
    // Both round away from zero; nonnumeric operands yield VALUE. No code,
    // data or tests reused. Array lifting follows the module's general contract.
    let book = book();
    let documented = RunConfiguration::default();
    for (function, expected) in [("EVEN", [2., -2., 2., -2.]), ("ODD", [3., -3., 3., -3.])] {
        for ((row, column), number) in [(1, 1), (1, 2), (2, 1), (2, 2)].into_iter().zip(expected) {
            let formula = format!("INDEX({function}({{1.5,-1.5;2,-2}}),{row},{column})");
            assert_eq!(
                value(run(&book, &formula, &documented, Default::default())),
                CellValue::Number(number),
                "{formula}"
            );
        }
        for (member, expected) in [
            (1, CellValue::Number(2. + f64::from(function == "ODD"))),
            (2, CellValue::Error(CellError::Value)),
            (3, CellValue::Error(CellError::NA)),
        ] {
            let formula = format!("INDEX({function}({{1.5,\"bad\",#N/A}}),1,{member})");
            assert_eq!(
                value(run(&book, &formula, &documented, Default::default())),
                expected,
                "{formula}"
            );
        }
        let formula = format!("IFERROR(SUM({function}(A1:B2)),99)");
        assert!(matches!(
            run(
                &book,
                &formula,
                &documented,
                EvaluationBudgets {
                    materialized_values: 2,
                    ..Default::default()
                }
            ),
            EvaluationOutcome::Aborted {
                failure: EvaluationFailure::Budget(_),
                ..
            }
        ));
    }
    let scalar =
        RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::ScalarIntersection);
    for (configuration, formula, expected) in [
        (&documented, "EVEN(0)", 0.),
        (&documented, "ODD(0)", 1.),
        (&documented, "SUM(ODD(EVEN({1.5,-1.5;2,-2})))", 0.),
        (&documented, "EVEN(@A1:A2)", 10.),
        (&documented, "ODD(@A1:A2)", 9.),
        (&scalar, "EVEN(A1:A2)", 2.),
        (&scalar, "ODD(A1:A2)", 1.),
        (&scalar, "SUM(EVEN(A1:A2))", 12.),
        (&scalar, "SUM(ODD(A1:A2))", 10.),
    ] {
        assert_eq!(
            value(run(&book, formula, configuration, Default::default())),
            CellValue::Number(expected),
            "{formula} ({configuration:?})"
        );
    }
}
