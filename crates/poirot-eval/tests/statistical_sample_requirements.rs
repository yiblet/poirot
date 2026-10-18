//! Independent sample/population equations and determined-fit boundary probes.
use poirot_eval::*;
use poirot_model::{CellAddress, CellError, CellValue, Workbook};

fn at() -> CellAddress {
    CellAddress { row: 99, col: 25 }
}
fn book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, value) in [
        ("A1", 1.),
        ("A2", 2.),
        ("A3", 3.),
        ("B1", 3.),
        ("B2", 5.),
        ("B3", 7.),
    ] {
        sheet.set_value(cell.parse().unwrap(), value).unwrap();
    }
    book
}

#[test]
fn statistical_sample_requirements_use_admitted_observations() {
    let book = book();
    for builtin in [
        BuiltinSemanticsPolicy::PinnedPoi,
        BuiltinSemanticsPolicy::ExcelDocumentation,
    ] {
        let config = RunConfiguration::default().with_builtin_policy(builtin);
        for (formula, excel, pinned) in [
            (
                "COVAR({2},{5})",
                CellValue::Number(0.),
                CellValue::Error(CellError::NA),
            ),
            (
                "COVARIANCE.P({\"skip\",2},{5,6})",
                CellValue::Number(0.),
                CellValue::Error(CellError::NA),
            ),
            (
                "COVARIANCE.S({2},{5})",
                CellValue::Error(CellError::Div0),
                CellValue::Error(CellError::NA),
            ),
            (
                "VARA({\"hello\"})",
                CellValue::Error(CellError::Div0),
                CellValue::Error(CellError::Num),
            ),
            (
                "STDEVA({FALSE})",
                CellValue::Error(CellError::Div0),
                CellValue::Error(CellError::Num),
            ),
            (
                "VARPA({\"hello\"})",
                CellValue::Number(0.),
                CellValue::Error(CellError::Num),
            ),
            ("VARA({0,0})", CellValue::Number(0.), CellValue::Number(0.)),
        ] {
            let expected = if builtin == BuiltinSemanticsPolicy::ExcelDocumentation {
                excel
            } else {
                pinned
            };
            let result = run_formula_with_configuration(
                &book,
                0,
                at(),
                formula,
                &config,
                Default::default(),
            );
            assert!(
                matches!(&result, EvaluationOutcome::Complete { value, .. } if value == &expected),
                "{builtin:?} {formula}: {result:?}"
            );
        }
    }
}

#[test]
fn prediction_requires_a_fit_without_residual_degrees_of_freedom() {
    let book = book();
    let policy = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&book, &policy);
    let config =
        RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation);
    for (formula, expected) in [
        ("TREND({3;5},{1;2},{3},TRUE)", 7.),
        ("TREND(B1:B2,A1:A2,{3},TRUE)", 7.),
        ("TREND({3},{1},{3},FALSE)", 9.),
        ("TREND({3;6;9},{1;2;3},{4},FALSE)", 12.),
        ("TREND({1;3;4},{0,0;1,0;0,1},{1,1},TRUE)", 6.),
        ("TREND({2;3},{1,0;0,1},{1,1},FALSE)", 5.),
        // Existing mean shortcut retained for intercept fits. Calc rejects this
        // rank-deficient input; this is an explicit Poirot compatibility contract.
        ("TREND({3;5},{1;1},{3},TRUE)", 4.),
        ("TREND({3;5;7},{1;1;1},{3},TRUE)", 5.),
        ("TREND({3;5},{1;1},{3},FALSE)", 12.),
    ] {
        let prepared = compiler.compile_formula(formula, &policy);
        let bound =
            bound::FormulaPlan::bind(&poirot_parsing::formula::parse_formula(formula).unwrap())
                .unwrap();
        let mut run = EvaluationRun::new_with_configuration(
            WorkbookView::new(&book, &plan, &policy).unwrap(),
            &config,
            Default::default(),
        )
        .unwrap();
        for value in [
            run.evaluate_formula(
                EvaluationTarget {
                    sheet: 0,
                    address: at(),
                },
                &prepared,
            )
            .unwrap(),
            run.evaluate_bound(
                EvaluationTarget {
                    sheet: 0,
                    address: at(),
                },
                &bound,
            )
            .unwrap(),
        ] {
            assert!(
                matches!(value, CellValue::Number(n) if (n-expected).abs() < 1e-9),
                "{formula}: {value:?}"
            );
        }
    }
    for formula in ["TREND({3},{1},{3},TRUE)", "TREND({3;5},{0;0},{3},FALSE)"] {
        assert!(
            matches!(
                run_formula_with_configuration(
                    &book,
                    0,
                    at(),
                    formula,
                    &config,
                    Default::default()
                ),
                EvaluationOutcome::Complete {
                    value: CellValue::Error(CellError::Value),
                    ..
                }
            ),
            "{formula}"
        );
    }
    // Preserve pinned errors and the legacy constant-predictor shortcut.
    assert!(matches!(
        run_formula(
            &book,
            0,
            at(),
            "TREND({3;5},{1;2},{3},TRUE)",
            Default::default(),
            Default::default()
        ),
        EvaluationOutcome::Complete {
            value: CellValue::Error(CellError::Ref),
            ..
        }
    ));
    assert!(matches!(
        run_formula(
            &book,
            0,
            at(),
            "TREND({3;5},{1;1},{3},FALSE)",
            Default::default(),
            Default::default()
        ),
        EvaluationOutcome::Complete {
            value: CellValue::Number(4.),
            ..
        }
    ));
}

#[test]
fn sample_size_failures_do_not_swallow_operational_limits() {
    let book = book();
    let config =
        RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation);
    for formula in [
        "IFERROR(COVARIANCE.P(A1:A3,B1:B3),99)",
        "IFERROR(TREND(B1:B2,A1:A2,{3},TRUE),99)",
        "IFERROR(VARA(A1:A3),99)",
    ] {
        for budgets in [
            EvaluationBudgets {
                materialized_values: 1,
                ..Default::default()
            },
            EvaluationBudgets {
                run_work: 1,
                ..Default::default()
            },
        ] {
            assert!(
                matches!(
                    run_formula_with_configuration(&book, 0, at(), formula, &config, budgets),
                    EvaluationOutcome::Aborted {
                        failure: EvaluationFailure::Budget(_),
                        ..
                    }
                ),
                "{formula}"
            );
        }
    }
}
