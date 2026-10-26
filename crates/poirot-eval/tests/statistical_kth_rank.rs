//! Modern LARGE and SMALL rank boundaries and collection contracts.
use poirot_eval::*;
use poirot_model::{CellAddress, CellError, CellValue, Workbook};

fn book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, value) in [
        ("A1", CellValue::Number(0.1)),
        ("A2", CellValue::Text("0.1".into())),
        ("A3", CellValue::Boolean(true)),
        ("B1", CellValue::Number(9.)),
        ("B2", CellValue::Number(2.)),
        ("B3", CellValue::Number(6.)),
    ] {
        sheet.set_value(cell.parse().unwrap(), value).unwrap();
    }
    book
}
fn at() -> CellAddress {
    CellAddress { row: 99, col: 25 }
}
fn config(policy: BuiltinSemanticsPolicy) -> RunConfiguration {
    RunConfiguration::default().with_builtin_policy(policy)
}

#[test]
fn kth_rank_admission_handles_fractional_boundaries() {
    let book = book();
    for builtin in [BuiltinSemanticsPolicy::ExcelDocumentation] {
        let config = config(builtin);
        for function in ["LARGE", "SMALL"] {
            let documented_large =
                function == "LARGE" && builtin == BuiltinSemanticsPolicy::ExcelDocumentation;
            for (rank, position) in [
                ("0.1", 1),
                ("0.999999", 1),
                ("1", 1),
                ("1.000001", 2),
                ("2.999999", 3),
                ("3", 3),
                ("3.000001", 0),
                ("0", 0),
                ("-0.1", 0),
                ("-1", 0),
                ("2147483648", 0),
                ("\"0.1\"", 1),
                ("A1", 1),
                ("A2", 1),
                ("{0.1}", 1),
                ("TRUE", 1),
                ("A3", 1),
                ("FALSE", 0),
                ("A4", 0),
            ] {
                let subunit = ["0.1", "0.999999", "\"0.1\"", "A1", "A2", "{0.1}"].contains(&rank);
                let expected = if position == 0 || (subunit && !documented_large) {
                    CellValue::Error(CellError::Num)
                } else {
                    CellValue::Number(match (function, position) {
                        ("LARGE", 1) | ("SMALL", 3) => 9.,
                        (_, 2) => 6.,
                        _ => 2.,
                    })
                };
                let formula = format!("{function}({{9,2,6}},{rank})");
                let result = run_formula_with_configuration(
                    &book,
                    0,
                    at(),
                    &formula,
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
}

#[test]
fn kth_rank_preserves_collection_error_order_and_unrelated_consumers() {
    let book = book();
    for builtin in [BuiltinSemanticsPolicy::ExcelDocumentation] {
        let config = config(builtin);
        for (formula, expected) in [
            ("LARGE(NA(),0.1)", CellValue::Error(CellError::NA)),
            ("LARGE(NA(),3.1)", CellValue::Error(CellError::NA)),
            ("LARGE({9,2,6},\"bad\")", CellValue::Error(CellError::Value)),
            ("PERCENTILE({2,6,9},0.25)", CellValue::Number(4.)),
            ("QUARTILE.INC({2,6,9},0.9)", CellValue::Number(2.)),
            ("RANK(6,B1:B3,0.1)", CellValue::Number(2.)),
            ("LARGE({9,\"100\",TRUE,2},1.1)", CellValue::Number(2.)),
            ("SMALL({9,\"100\",TRUE,2},1.1)", CellValue::Number(9.)),
        ] {
            let result = run_formula_with_configuration(
                &book,
                0,
                at(),
                formula,
                &config,
                Default::default(),
            );
            assert!(
                matches!(&result, EvaluationOutcome::Complete {value,..} if value == &expected),
                "{builtin:?} {formula}: {result:?}"
            );
        }
    }
}

#[test]
fn kth_rank_admission_retains_operational_budgets() {
    let book = book();
    let config = config(BuiltinSemanticsPolicy::ExcelDocumentation);
    for formula in [
        "IFERROR(LARGE(B1:B3,0.1),99)",
        "IFERROR(SMALL(B1:B3,1.1),99)",
        "IFERROR(LARGE(B1:B3,\"0.1\"),99)",
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
