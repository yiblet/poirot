//! Exact predicate types and rectangular demand are independent of numeric coercion.
use poirot_eval::{
    overlay::{EvaluationContext, Overlay, OverlayPolicy},
    *,
};
use poirot_model::{CellError, CellValue, ExcelText, Workbook};
fn target(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}
fn book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, value) in [
        ("A1", CellValue::Number(1.)),
        (
            "B1",
            CellValue::TextUtf16(ExcelText::from_units(vec![b'2' as u16])),
        ),
        ("C1", CellValue::Text("".into())),
        ("A2", CellValue::Boolean(true)),
        ("B2", CellValue::Error(CellError::NA)),
        ("C2", CellValue::Error(CellError::Div0)),
        ("D2", CellValue::Number(0.)),
    ] {
        sheet.set_value(target(cell).address, value).unwrap();
    }
    book
}
fn configuration(arrays: ArrayResultPolicy) -> RunConfiguration {
    RunConfiguration::default()
        .with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation)
        .with_array_result_policy(arrays)
}
fn result(
    book: &Workbook,
    at: &str,
    formula: &str,
    arrays: ArrayResultPolicy,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        book,
        0,
        target(at).address,
        formula,
        &configuration(arrays),
        budgets,
    )
}
fn value(outcome: EvaluationOutcome<CellValue>) -> CellValue {
    match outcome {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
fn expect(book: &Workbook, formula: &str, arrays: ArrayResultPolicy, expected: CellValue) {
    assert_eq!(
        value(result(
            book,
            "Z100",
            formula,
            arrays,
            EvaluationBudgets::default()
        )),
        expected,
        "{arrays:?}: {formula}"
    );
}
#[test]
fn mixed_rectangles_inspect_types_without_converting_numbers_text_blanks_booleans_or_errors() {
    // Microsoft IS functions explicitly inspect types without conversion. Fresh Calc
    // corroborates text/blank/error members; Calc array mode converts logical members
    // to numbers, unlike its scalar mode. That disagreement is retained in evidence.
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        for (function, expected) in [
            ("ISERROR", 96.),
            ("ISERR", 64.),
            ("ISNA", 32.),
            ("ISNUMBER", 129.),
            ("ISTEXT", 6.),
            ("ISBLANK", 8.),
            ("ISLOGICAL", 16.),
            ("ISNONTEXT", 249.),
        ] {
            expect(
                &book(),
                &format!("SUMPRODUCT(--{function}(A1:D2),{{1,2,4,8;16,32,64,128}})"),
                arrays,
                CellValue::Number(expected),
            );
        }
        for (formula, expected) in [
            (
                "SUMPRODUCT(--ISNUMBER({1,\"2\",FALSE;3,\"\",TRUE}),{1,2,4;8,16,32})",
                9.,
            ),
            (
                "SUMPRODUCT(--ISLOGICAL({TRUE,FALSE,1,\"TRUE\"}),{1,2,4,8})",
                3.,
            ),
            ("SUMPRODUCT(--ISERROR({1,#N/A;#DIV/0!,4}),{1,2;4,8})", 6.),
            ("SUMPRODUCT(--ISNA({1,#N/A;#DIV/0!,4}),{1,2;4,8})", 2.),
        ] {
            expect(&book(), formula, arrays, CellValue::Number(expected));
        }
    }
}
#[test]
fn scalar_projection_and_missing_rules_survive_array_support() {
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        for function in [
            "ISERROR",
            "ISERR",
            "ISNA",
            "ISNUMBER",
            "ISTEXT",
            "ISBLANK",
            "ISLOGICAL",
            "ISNONTEXT",
        ] {
            expect(
                &book(),
                &format!("{function}(IFS(TRUE,))"),
                arrays,
                CellValue::Boolean(function == "ISNONTEXT"),
            );
            expect(
                &book(),
                &format!("{function}()"),
                arrays,
                CellValue::Error(CellError::Value),
            );
        }
        for (formula, expected) in [
            ("ISNUMBER(\"1\")", false),
            ("ISLOGICAL(TRUE())", true),
            ("ISBLANK(D1)", true),
            ("ISTEXT(C1)", true),
            ("ISBLANK(IF(TRUE,D1,0))", true),
            ("ISBLANK(IF(TRUE,,0))", true),
        ] {
            expect(&book(), formula, arrays, CellValue::Boolean(expected));
        }
        // Omitted literal array members remain invalid parser syntax.
        expect(
            &book(),
            "ISBLANK({,1})",
            arrays,
            CellValue::Error(CellError::Value),
        );
    }
    let arrays = ArrayResultPolicy::ScalarIntersection;
    for (at, formula, expected) in [
        ("B2", "ISNA(A1:B2)", true),
        ("A1", "ISNUMBER(A1:B2)", true),
        ("Z100", "ISERROR(A1:B2)", true),
        ("Z100", "ISNUMBER(A1:B2)", false),
    ] {
        assert_eq!(
            value(result(
                &book(),
                at,
                formula,
                arrays,
                EvaluationBudgets::default()
            )),
            CellValue::Boolean(expected)
        );
    }
    // SUM's reference-expression demand remains scalar; SUMPRODUCT supplies array demand.
    expect(
        &book(),
        "SUM(--ISNUMBER(A1:D2))",
        arrays,
        CellValue::Number(0.),
    );
}
#[test]
fn pinned_predicate_array_projection_and_nonpredicate_information_functions_are_unchanged() {
    for (formula, expected) in [
        ("ISNA({7,\"text\";TRUE,#N/A})", CellValue::Boolean(true)),
        ("TYPE({1,2})", CellValue::Number(1.)),
        ("N({TRUE,FALSE})", CellValue::Number(1.)),
        ("T({\"x\",1})", CellValue::Text("x".into())),
        ("ISREF(A1:D2)", CellValue::Boolean(true)),
    ] {
        let at = if formula.starts_with("ISNA") {
            "B2"
        } else {
            "Z100"
        };
        assert_eq!(
            value(run_formula_with_configuration(
                &book(),
                0,
                target(at).address,
                formula,
                &RunConfiguration::default()
                    .with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi)
                    .with_array_result_policy(ArrayResultPolicy::ScalarIntersection),
                EvaluationBudgets::default()
            )),
            expected,
            "{formula}"
        );
        if !formula.starts_with("ISNA") {
            expect(
                &book(),
                formula,
                ArrayResultPolicy::ScalarIntersection,
                expected,
            );
        }
    }
}
#[test]
fn predicate_spills_preserve_both_axes_and_are_discovered_before_the_anchor() {
    for formula in [
        "ISNUMBER(A1:D2)",
        "ISNUMBER(ISNUMBER(A1:D2))",
        "ISLOGICAL({TRUE})",
    ] {
        let mut book = book();
        book.sheet_mut(0)
            .unwrap()
            .set_formula(target("F5").address, formula, CellValue::Blank)
            .unwrap();
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(OverlayPolicy {
                arrays: ArrayResultPolicy::DynamicSpill,
                builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
                ..Default::default()
            })
            .build()
            .unwrap();
        let context = EvaluationContext::default();
        let checks = if formula.contains("ISNUMBER(ISNUMBER") {
            vec![("I6", false), ("F5", false), ("G5", false)]
        } else if formula.starts_with("ISLOGICAL") {
            vec![("F5", true)]
        } else {
            vec![("I6", true), ("F6", false), ("G5", false), ("F5", true)]
        };
        for (cell, expected) in checks {
            assert_eq!(
                overlay
                    .evaluation(&context)
                    .unwrap()
                    .cell(target(cell))
                    .unwrap(),
                CellValue::Boolean(expected),
                "{formula}: {cell}"
            );
        }
    }
}
#[test]
fn reference_edits_and_builtin_policy_changes_invalidate_retained_predicate_results() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            target("F5").address,
            "SUMPRODUCT(--ISNUMBER(A1:D2),{1,2,4,8;16,32,64,128})",
            CellValue::Blank,
        )
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(OverlayPolicy {
            builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("F5"))
            .unwrap(),
        CellValue::Number(129.)
    );
    let (book, state) = Overlay::from_snapshot(overlay.snapshot()).unwrap();
    let mut overlay = state.attach(&book).unwrap();
    overlay.set_value(target("B1"), 2.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("F5"))
            .unwrap(),
        CellValue::Number(131.)
    );
    for (builtin, expected) in [
        (
            BuiltinSemanticsPolicy::PinnedPoi,
            CellValue::Error(CellError::Value),
        ),
        (
            BuiltinSemanticsPolicy::ExcelDocumentation,
            CellValue::Number(131.),
        ),
        (
            BuiltinSemanticsPolicy::PinnedPoi,
            CellValue::Error(CellError::Value),
        ),
    ] {
        overlay
            .set_policy(OverlayPolicy {
                builtin,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("F5"))
                .unwrap(),
            expected
        );
    }
}
#[test]
fn spreadsheet_errors_are_data_but_operational_failures_and_limits_escape_iferror() {
    let mut workbook = book();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(
            target("D1").address,
            "COUNTIF(A1:A2,B1:B2)",
            CellValue::Blank,
        )
        .unwrap();
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        assert!(matches!(
            result(
                &workbook,
                "Z100",
                "IFERROR(SUMPRODUCT(--ISERROR(A1:D2)),99)",
                arrays,
                EvaluationBudgets::default()
            ),
            EvaluationOutcome::Aborted { .. }
        ));
        assert!(matches!(
            result(
                &book(),
                "Z100",
                "IFERROR(SUMPRODUCT(--ISNUMBER(A1:D2)),99)",
                arrays,
                EvaluationBudgets {
                    materialized_values: 7,
                    ..Default::default()
                }
            ),
            EvaluationOutcome::Aborted { .. }
        ));
    }
}
#[test]
fn a_compound_operand_is_evaluated_once_and_not_once_per_member() {
    let outcome = result(
        &book(),
        "Z100",
        "SUMPRODUCT(--ISNUMBER(IF({TRUE,TRUE,TRUE},RAND(),UNKNOWN())))",
        ArrayResultPolicy::ScalarIntersection,
        EvaluationBudgets::default(),
    );
    assert_eq!(outcome.usage().random_accesses.first().unwrap().draws, 1);
    assert_eq!(value(outcome), CellValue::Number(3.));
}

#[test]
fn singleton_predicate_arrays_publish_a_boolean_cell_without_becoming_references() {
    let mut book = book();
    let sheet = book.sheet_mut(0).unwrap();
    sheet
        .set_formula(target("F5").address, "ISLOGICAL({TRUE})", CellValue::Blank)
        .unwrap();
    sheet
        .set_formula(
            target("H5").address,
            "ISREF(ISLOGICAL({TRUE}))",
            CellValue::Blank,
        )
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(OverlayPolicy {
            arrays: ArrayResultPolicy::DynamicSpill,
            builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    for (cell, expected) in [
        ("F5", CellValue::Boolean(true)),
        ("G5", CellValue::Blank),
        ("H5", CellValue::Boolean(false)),
    ] {
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target(cell))
                .unwrap(),
            expected,
            "{cell}"
        );
    }
}

#[test]
fn named_array_footprints_remain_unsupported_at_publication_and_child_discovery() {
    let mut book = book();
    book.define_name("Inputs", "A1:D2", None).unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(target("F5").address, "ISNUMBER(Inputs)", CellValue::Blank)
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(OverlayPolicy {
            arrays: ArrayResultPolicy::DynamicSpill,
            builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    // Both discovery and publication retain the established static-bound contract.
    for cell in ["I6", "F5"] {
        assert!(
            matches!(
                overlay.evaluation(&context).unwrap().cell(target(cell)),
                Err(EvaluationFailure::UnsupportedCapability { .. })
            ),
            "{cell}"
        );
    }
}
