//! Public controls for moved calendar, reference and text materialization paths.
use poirot_eval::*;
use poirot_model::{CellAddress, CellError, CellValue, Workbook};

/// Build a workbook with consecutive weekday holiday serials.
fn book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (row, serial) in [(0, 44467.), (1, 44468.)] {
        sheet
            .set_value(CellAddress { row, col: 0 }, serial)
            .unwrap();
    }
    book
}

/// Evaluate one formula through the public run interface.
fn outcome(
    book: &Workbook,
    formula: &str,
    budgets: EvaluationBudgets,
    excel: bool,
) -> EvaluationOutcome<CellValue> {
    let config = if excel {
        RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation)
    } else {
        RunConfiguration::default()
    };
    run_formula_with_configuration(
        book,
        0,
        CellAddress { row: 9, col: 9 },
        formula,
        &config,
        budgets,
    )
}

#[test]
fn moved_paths_preserve_public_values() {
    let book = book();
    // Calendar controls use the same September 2021 weekdays documented in the
    // existing date tests; text/reference expectations follow the literal operands.
    for excel in [false, true] {
        for (formula, expected) in [
            ("NETWORKDAYS(44466,44470,A1:A2)", CellValue::Number(3.)),
            (
                "NETWORKDAYS(44466,44470,{44467;44468})",
                CellValue::Number(3.),
            ),
            ("WORKDAY(44466,1,A1:A2)", CellValue::Number(44469.)),
            (
                "WORKDAY(44466,-9223372036854775808)",
                CellValue::Error(CellError::Num),
            ),
            ("INDEX({1,2;3,4},2,2)", CellValue::Number(4.)),
            ("SUM(INDEX({1,2;3,4},0,2))", CellValue::Number(6.)),
            ("MEDIAN(1,4,2)", CellValue::Number(2.)),
            ("TRIM(\"  a   b  \")", CellValue::Text("a b".into())),
            ("CLEAN(\"a\"&CHAR(10)&\"b\")", CellValue::Text("ab".into())),
            (
                "SUBSTITUTE(\"aaa\",\"a\",\"xy\")",
                CellValue::Text("xyxyxy".into()),
            ),
            (
                "SUBSTITUTE(\"aaa\",\"a\",\"xy\",2)",
                CellValue::Text("axya".into()),
            ),
            ("\"a\"&\"😀\"", CellValue::Text("a😀".into())),
        ] {
            match outcome(&book, formula, EvaluationBudgets::default(), excel) {
                EvaluationOutcome::Complete { value, .. } => {
                    assert_eq!(value, expected, "{formula}, excel={excel}")
                }
                other => panic!("{formula}, excel={excel}: {other:?}"),
            }
        }
    }
}

#[test]
fn moved_collection_limits_remain_operational_and_attributed() {
    let book = book();
    for formula in [
        "IFERROR(NETWORKDAYS(44466,44470,A1:A2),99)",
        "IFERROR(MEDIAN(A1:A2),99)",
        "IFERROR(SUM(INDEX(A1:A2,0,1)),99)",
    ] {
        match outcome(
            &book,
            formula,
            EvaluationBudgets {
                materialized_values: 1,
                ..Default::default()
            },
            true,
        ) {
            EvaluationOutcome::Aborted {
                failure: EvaluationFailure::Budget(failure),
                ..
            } => {
                assert_eq!(
                    failure.target,
                    Some(EvaluationTarget {
                        sheet: 0,
                        address: CellAddress { row: 9, col: 9 }
                    })
                );
            }
            other => panic!("{formula}: {other:?}"),
        }
    }
}
