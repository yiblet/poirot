//! Public logical contracts from Microsoft's IFERROR, IFNA, NOT, and logical
//! function documentation; controls are independently authored, with no reused code.
//! https://support.microsoft.com/en-us/office/iferror-function-c526fd07-caeb-47b8-8bb6-63f3e417f611
//! https://support.microsoft.com/en-us/office/ifna-function-6626c961-a569-42fc-a49d-79b4951fd461
//! Invalid arity is rejected before argument evaluation. Reference channels stay
//! intact when a handler has no errors to replace.
use poirot_eval::{EvaluationBudgets, EvaluationOutcome, run_formula};
use poirot_model::{CellError, CellValue, Workbook};

fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet.set_value("A1".parse().unwrap(), 2.).unwrap();
    sheet.set_value("A2".parse().unwrap(), 3.).unwrap();
    sheet
        .set_value(
            "A3".parse().unwrap(),
            CellValue::Error(CellError::GettingData),
        )
        .unwrap();
    sheet.set_formula("A4".parse().unwrap(), "RAND()").unwrap();
    workbook
}

fn evaluate(formula: &str, budgets: EvaluationBudgets) -> EvaluationOutcome<CellValue> {
    run_formula(
        &workbook(),
        0,
        "Z1".parse().unwrap(),
        formula,
        Default::default(),
        budgets,
    )
}

#[test]
fn error_handlers_retain_selected_channels_and_skip_uncaught_fallbacks() {
    for (formula, expected) in [
        ("SUM(IFERROR(A1:A2,RAND()))", CellValue::Number(5.)),
        ("SUM(IFERROR(SQRT({4,-1;9,16}),0))", CellValue::Number(9.)),
        ("SUM(IFERROR({1,#N/A},0))", CellValue::Number(1.)),
        ("SUM(IFNA({1,#N/A},0))", CellValue::Number(1.)),
        ("SUM(IFERROR({#N/A,2},{3,4}))", CellValue::Number(5.)),
        ("SUM(IFERROR(#N/A,{2,3}))", CellValue::Number(5.)),
        ("ISREF(IFERROR(A1:A2,RAND()))", CellValue::Boolean(true)),
        ("ISREF(IFERROR(#N/A,A1:A2))", CellValue::Boolean(true)),
        ("ISREF(A4)", CellValue::Boolean(true)),
        ("IFNA({#DIV/0!,#N/A},0)", CellValue::Error(CellError::Div0)),
        ("SUM(IFNA(A1:A2,RAND()))", CellValue::Number(5.)),
        ("SUM(IFERROR({2,3},RAND()))", CellValue::Number(5.)),
        ("SUM(IFERROR(#N/A,A1:A2))", CellValue::Number(5.)),
        ("SUM(IFNA(#N/A,A1:A2))", CellValue::Number(5.)),
        ("IFNA(#REF!,RAND())", CellValue::Error(CellError::Ref)),
        ("SUM(IFERROR(,RAND()))", CellValue::Number(0.)),
        ("IFERROR(#DIV/0!,7)", CellValue::Number(7.)),
    ] {
        let outcome = evaluate(formula, Default::default());
        assert!(outcome.usage().random_accesses.is_empty(), "{formula}");
        assert!(
            matches!(&outcome, EvaluationOutcome::Complete { value, .. } if *value == expected),
            "{formula}: {outcome:?}"
        );
    }
}

#[test]
fn scalar_arity_rejects_without_forcing_invalid_operands() {
    for (formula, expected, draws) in [
        ("TRUE(RAND())", CellValue::Error(CellError::Value), 0),
        ("FALSE(RAND())", CellValue::Error(CellError::Value), 0),
        ("IFERROR(RAND())", CellValue::Error(CellError::Value), 0),
        ("IFNA(RAND())", CellValue::Error(CellError::Value), 0),
        ("NOT(#REF!,RAND())", CellValue::Error(CellError::Value), 0),
        ("NOT(RAND(),RAND())", CellValue::Error(CellError::Value), 0),
        ("NOT(0)", CellValue::Boolean(true), 0),
        ("NOT()", CellValue::Error(CellError::Value), 0),
        ("TRUE()", CellValue::Boolean(true), 0),
        ("FALSE()", CellValue::Boolean(false), 0),
        ("AND(TRUE,0)", CellValue::Boolean(false), 0),
        ("OR(FALSE,1)", CellValue::Boolean(true), 0),
        ("XOR(TRUE,TRUE,TRUE)", CellValue::Boolean(true), 0),
    ] {
        let outcome = evaluate(formula, Default::default());
        assert_eq!(
            outcome
                .usage()
                .random_accesses
                .iter()
                .map(|access| access.draws)
                .sum::<u64>(),
            draws,
            "{formula}"
        );
        assert!(
            matches!(&outcome, EvaluationOutcome::Complete { value, .. } if *value == expected),
            "{formula}: {outcome:?}"
        );
    }
}

#[test]
fn error_handlers_do_not_catch_operational_budget_failures() {
    for formula in [
        "IFERROR(NOT(SUM(SEQUENCE(100))),7)",
        "IFNA(SUM(SEQUENCE(100)),7)",
        "IFERROR(AND(TRUE,SUM(SEQUENCE(100))),7)",
        "IFERROR(OR(FALSE,SUM(SEQUENCE(100))),7)",
        "IFERROR(XOR(TRUE,SUM(SEQUENCE(100))),7)",
    ] {
        assert!(
            matches!(
                evaluate(
                    formula,
                    EvaluationBudgets {
                        run_work: 3,
                        ..Default::default()
                    }
                ),
                EvaluationOutcome::Aborted { .. }
            ),
            "{formula}"
        );
    }
}

/// Independently authored cases from Microsoft's ISEVEN and ERROR.TYPE tables.
/// https://support.microsoft.com/en-us/excel/functions/iseven-function
/// https://support.microsoft.com/en-us/excel/functions/error-type-function
#[test]
fn scalar_and_broadcast_logical_kernels_follow_documented_contracts() {
    for (formula, expected) in [
        ("ISEVEN(-1)", CellValue::Boolean(false)),
        ("ISEVEN(2.5)", CellValue::Boolean(true)),
        ("ISODD(-3.9)", CellValue::Boolean(true)),
        ("ISEVEN(\"nonnumeric\")", CellValue::Error(CellError::Value)),
        ("SUM(--ISEVEN({2,3;4,5}))", CellValue::Number(2.)),
        ("SUM(--NOT({0,1;0,3}))", CellValue::Number(2.)),
        ("SUM(ERROR.TYPE({#DIV/0!,#N/A}))", CellValue::Number(9.)),
        ("ERROR.TYPE(1)", CellValue::Error(CellError::NA)),
        ("ERROR.TYPE(A3)", CellValue::Number(8.)),
        ("ISREF(A1:A2)", CellValue::Boolean(true)),
        ("ISREF({1,2})", CellValue::Boolean(false)),
        ("ISREF(1/0)", CellValue::Boolean(false)),
    ] {
        let outcome = evaluate(formula, Default::default());
        assert!(
            matches!(&outcome, EvaluationOutcome::Complete {value, ..} if *value == expected),
            "{formula}: {outcome:?}"
        );
    }
}

/// Edits to selected fallback cells invalidate recovery without reading inactive cells.
#[test]
fn recovery_overlay_tracks_source_and_selected_fallback_members() {
    use poirot_eval::EvaluationTarget;
    use poirot_eval::overlay::{EvaluationContext, Overlay};
    let target = |address: &str| EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    };
    let mut book = workbook();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_value(target("B1").address, 7.).unwrap();
    sheet.set_value(target("B2").address, 11.).unwrap();
    sheet
        .set_formula(target("C1").address, "SUM(IFERROR(A1:A2,B1:B2))")
        .unwrap();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C1"))
            .unwrap(),
        CellValue::Number(5.)
    );
    overlay.set_value(target("B1"), 20.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C1"))
            .unwrap(),
        CellValue::Number(5.)
    );
    overlay
        .set_value(target("A2"), CellValue::Error(CellError::NA))
        .unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C1"))
            .unwrap(),
        CellValue::Number(13.)
    );
    overlay.set_value(target("B2"), 17.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C1"))
            .unwrap(),
        CellValue::Number(19.)
    );
    overlay.set_value(target("A2"), 3.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C1"))
            .unwrap(),
        CellValue::Number(5.)
    );
}

/// Fallback references contribute values only at caught source coordinates.
#[test]
fn recovery_skips_uncaught_fallback_reference_cells() {
    let mut book = workbook();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_value("B1".parse().unwrap(), 7.).unwrap();
    sheet.set_formula("B2".parse().unwrap(), "RAND()").unwrap();
    let outcome = run_formula(
        &book,
        0,
        "Z1".parse().unwrap(),
        "SUM(IFERROR({#N/A;2},B1:B2))",
        Default::default(),
        Default::default(),
    );
    assert!(outcome.usage().random_accesses.is_empty());
    assert!(matches!(
        outcome,
        EvaluationOutcome::Complete {
            value: CellValue::Number(9.),
            ..
        }
    ));
}

#[test]
fn variadic_selectors_preserve_scalar_references_before_unused_array_probes() {
    // First-true/first-match and positional selection follow the documented
    // contracts recorded in ifs/choose/switch-array-selection-probes.json.
    // ISREF independently checks the selected channel without reading A4's RAND.
    for formula in [
        "ISREF(IFS(FALSE,RAND(),TRUE,A1:A2,{TRUE,FALSE},RAND()))",
        "ISREF(SWITCH(2,1,RAND(),2,A1:A2,{2,3},RAND()))",
        "ISREF(CHOOSE(2,RAND(),A1:A2,RAND()))",
    ] {
        let outcome = evaluate(formula, Default::default());
        assert!(outcome.usage().random_accesses.is_empty(), "{formula}");
        assert!(
            matches!(
                outcome,
                EvaluationOutcome::Complete {
                    value: CellValue::Boolean(true),
                    ..
                }
            ),
            "{formula}: {outcome:?}"
        );
    }
}

#[test]
fn late_variadic_probe_expansion_replicates_settled_coordinates() {
    // Existing selection probes establish singleton-axis broadcasting and
    // first-match order. The sums below are hand calculated from those rules:
    // IFS/SWITCH yield {2,3;2,5}; CHOOSE yields {2,3;4,5}.
    let configuration = poirot_eval::RunConfiguration::default()
        .with_builtin_policy(poirot_eval::BuiltinSemanticsPolicy::ExcelDocumentation);
    for (formula, expected) in [
        ("SUM(IFS({TRUE,FALSE},2,{TRUE;FALSE},{3;4},TRUE,5))", 12.),
        ("SUM(SWITCH({1,2},1,2,{2;3},{3;4},5))", 12.),
        ("SUM(CHOOSE({1,2},{2;4},{3;5},RAND()))", 14.),
    ] {
        let outcome = poirot_eval::run_formula_with_configuration(
            &workbook(),
            0,
            "Z1".parse().unwrap(),
            formula,
            &configuration,
            Default::default(),
        );
        assert!(outcome.usage().random_accesses.is_empty(), "{formula}");
        assert!(
            matches!(outcome, EvaluationOutcome::Complete { value: CellValue::Number(value), .. } if value == expected),
            "{formula}: {outcome:?}"
        );
    }
}
