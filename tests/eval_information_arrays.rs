//! Exact predicate types and rectangular demand are independent of numeric coercion.
use poirot::eval::{
    overlay::{EvaluationContext, EvaluationPolicy, Overlay},
    *,
};
use poirot::model::{CellError, CellValue, ExcelText, Workbook};
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
fn information_host_classifies_selected_errors_without_forcing_unselected_branches() {
    // Microsoft documents ISERR as excluding #N/A and TYPE as reporting errors as 16.
    for (formula, expected) in [
        ("ISERROR(IF(FALSE,1/0,5))", CellValue::Boolean(false)),
        ("ISERROR(IF(TRUE,1/0,5))", CellValue::Boolean(true)),
        ("ISERR(NA())", CellValue::Boolean(false)),
        ("ISNA(NA())", CellValue::Boolean(true)),
        ("TYPE(NA())", CellValue::Number(16.)),
    ] {
        expect(
            &book(),
            formula,
            ArrayResultPolicy::ScalarIntersection,
            expected,
        );
    }
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
        ("B2", "ISNA(A1:B2)", false),
        ("A1", "ISNUMBER(A1:B2)", true),
        ("Z100", "ISERROR(A1:B2)", false),
        ("Z100", "ISNUMBER(A1:B2)", true),
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
    // Typed predicates produce the same rectangle when nested in an aggregate.
    expect(
        &book(),
        "SUM(--ISNUMBER(A1:D2))",
        arrays,
        CellValue::Number(2.),
    );
}
#[test]
fn typed_information_shapes_are_independent_of_legacy_builtin_policy() {
    for (formula, expected) in [
        ("ISNA({7,\"text\";TRUE,#N/A})", CellValue::Boolean(false)),
        ("TYPE({1,2})", CellValue::Number(64.)),
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
            .set_formula(target("F5").address, formula)
            .unwrap();
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(EvaluationPolicy {
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
        )
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
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
        (BuiltinSemanticsPolicy::PinnedPoi, CellValue::Number(131.)),
        (
            BuiltinSemanticsPolicy::ExcelDocumentation,
            CellValue::Number(131.),
        ),
        (BuiltinSemanticsPolicy::PinnedPoi, CellValue::Number(131.)),
    ] {
        overlay
            .set_policy(EvaluationPolicy {
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
        .set_formula(target("D1").address, "COUNTIF(A1:A2,)")
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
        .set_formula(target("F5").address, "ISLOGICAL({TRUE})")
        .unwrap();
    sheet
        .set_formula(target("H5").address, "ISREF(ISLOGICAL({TRUE}))")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
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
fn named_array_footprints_publish_with_child_first_reads() {
    let mut book = book();
    book.define_name("Inputs", "A1:D2", None).unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(target("F5").address, "ISNUMBER(Inputs)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
            arrays: ArrayResultPolicy::DynamicSpill,
            builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    for cell in ["I6", "F5"] {
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target(cell))
                .unwrap(),
            CellValue::Boolean(true),
            "{cell}"
        );
    }
}

#[test]
fn typed_information_preserves_source_identity_and_broadcasts_conversions() {
    // TYPE's scalar codes and array-constant 64 are documented by Microsoft:
    // https://support.microsoft.com/en-us/excel/functions/type-function
    // Multi-cell references describe the same modern array value without selecting a member.
    let book = book();
    assert_eq!(
        value(result(
            &book,
            "A1",
            "TYPE(@A1:A2)",
            ArrayResultPolicy::DynamicSpill,
            EvaluationBudgets::default()
        )),
        CellValue::Number(1.)
    );
    for (formula, expected) in [
        ("TYPE(A1)", CellValue::Number(1.)),
        ("TYPE(B1)", CellValue::Number(2.)),
        ("TYPE({1,2;3,4})", CellValue::Number(64.)),
        ("TYPE(A1:B2)", CellValue::Number(64.)),
        ("TYPE(NA())", CellValue::Number(16.)),
        ("SUM(N({TRUE,FALSE;2,3}))", CellValue::Number(6.)),
        (
            "TEXTJOIN(\"\",TRUE,T({1,\"a\";FALSE,\"b\"}))",
            CellValue::Text("ab".into()),
        ),
    ] {
        expect(&book, formula, ArrayResultPolicy::DynamicSpill, expected);
    }
}
