//! Original public tests based on Microsoft contracts, retrieved 2026-10-21.
//! https://support.microsoft.com/en-us/excel/functions/hstack-function
//! https://support.microsoft.com/en-us/excel/functions/vstack-function
//! https://support.microsoft.com/en-us/excel/functions/take-function
//! https://support.microsoft.com/en-us/excel/functions/drop-function
//! https://support.microsoft.com/en-us/excel/functions/choosecols-function
//! https://support.microsoft.com/en-us/excel/functions/chooserows-function
//! https://support.microsoft.com/en-us/excel/functions/expand-function
use poirot_eval::{
    overlay::{Evaluation, EvaluationContext, Overlay, OverlayPolicy},
    *,
};
use poirot_model::{CellError, CellValue, Workbook};
fn at(s: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: s.parse().unwrap(),
    }
}
fn policy() -> OverlayPolicy {
    OverlayPolicy {
        builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
        ..Default::default()
    }
}
fn formula(text: &str) -> CellValue {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    match run_formula_with_configuration(
        &book,
        0,
        at("Z100").address,
        text,
        &RunConfiguration::default()
            .with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation),
        EvaluationBudgets::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
#[test]
fn signed_counts_omissions_repeats_and_nested_arrays() {
    for (text, n) in [
        ("SUM(EXPAND(2,))", 2.),
        ("SUM(TAKE({2,4,6;8,10,12},,-2))", 32.),
        ("SUM(TAKE({2,4,6;8,10,12},-1))", 30.),
        ("SUM(TAKE({2,4,6;8,10,12},99))", 42.),
        ("SUM(DROP({2,4,6;8,10,12},,2))", 18.),
        ("SUM(DROP({2,4,6;8,10,12},-1))", 12.),
        ("SUM(CHOOSECOLS({2,4,6;8,10,12},-1,1,-1))", 46.),
        ("SUM(CHOOSEROWS({2,4,6;8,10,12},-1,1))", 42.),
        ("SUM(CHOOSECOLS({2,4,6;8,10,12},{3,1}))", 28.),
        ("SUM(EXPAND({2,4;8,10},3,3,0))", 24.),
        ("SUM(EXPAND({2,4;8,10},,3,2))", 28.),
        ("SUM(HSTACK(TAKE({2,4;8,10},1),{16,32}))", 54.),
        ("SUM(TAKE({2,4,6;8,10,12},1.9))", 12.),
    ] {
        assert_eq!(formula(text), CellValue::Number(n), "{text}");
    }
}
#[test]
fn error_domains_and_padding_are_spreadsheet_values() {
    for (text, error) in [
        ("TAKE({2,4},0)", CellError::Calc),
        ("DROP({2,4},1)", CellError::Calc),
        ("CHOOSECOLS({2,4},0)", CellError::Value),
        ("CHOOSEROWS({2;4},3)", CellError::Value),
        ("EXPAND({2,4},1,1)", CellError::Value),
        ("EXPAND(2,1048577)", CellError::Num),
        ("SUM(HSTACK({2;4},8))", CellError::NA),
        ("SUM(VSTACK({2,4},8))", CellError::NA),
        ("SUM(EXPAND(2,2))", CellError::NA),
    ] {
        assert_eq!(formula(text), CellValue::Error(error), "{text}");
    }
    assert_eq!(formula("INDEX(HSTACK({2;4},8),1,2)"), 8.0.into());
    assert_eq!(formula("INDEX(VSTACK({2,4},8),2,1)"), 8.0.into());
}
#[test]
fn mixed_cells_spill_children_and_obstructions_work_in_both_evaluators() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value(at("A1").address, "blue").unwrap();
    sheet.set_value(at("B1").address, true).unwrap();
    for (cell, text) in [
        ("D1", "HSTACK(A1:B2,{7;9})"),
        ("H1", "VSTACK(A1:B1,{5})"),
        ("L1", "TAKE({2,4;8,10},1)"),
        ("P1", "EXPAND(2,2,2,0)"),
    ] {
        sheet.set_formula(at(cell).address, text).unwrap();
    }
    sheet.set_value(at("Q2").address, 99.).unwrap();
    for reverse in [false, true] {
        let mut direct = Evaluation::with_workbook(&book)
            .with_policy(policy())
            .build(Default::default())
            .unwrap();
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(policy())
            .build()
            .unwrap();
        let context = EvaluationContext::default();
        let mut e = overlay.evaluation(&context).unwrap();
        let mut expected = vec![
            ("D1", CellValue::Text("blue".into())),
            ("E1", true.into()),
            ("D2", 0.0.into()),
            ("F2", 9.0.into()),
            ("H2", 5.0.into()),
            ("I2", CellValue::Error(CellError::NA)),
            ("M1", 4.0.into()),
            ("P1", CellValue::Error(CellError::Spill)),
        ];
        if reverse {
            expected.reverse();
        }
        for (cell, value) in expected {
            assert_eq!(direct.cell(at(cell)).unwrap(), value, "direct {cell}");
            assert_eq!(e.cell(at(cell)).unwrap(), value, "overlay {cell}");
        }
    }
}
#[test]
fn overlay_dimension_edits_replace_old_spills() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_formula(at("A1").address, "EXPAND(7,J1,2,0)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(book)
        .with_policy(policy())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    for (rows, expected) in [
        (3, 0.0.into()),
        (1, CellValue::Blank),
        (2, CellValue::Blank),
        (3, 0.0.into()),
    ] {
        overlay.set_value(at("J1"), rows as f64).unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("B3"))
                .unwrap(),
            expected
        );
    }
}
#[test]
fn budgets_abort_before_expansion_and_cannot_be_caught() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    assert!(matches!(
        run_formula_with_configuration(
            &book,
            0,
            at("A1").address,
            "IFERROR(SUM(EXPAND(1,1000,1000,0)),99)",
            &RunConfiguration::default()
                .with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation),
            EvaluationBudgets {
                materialized_values: 100,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}
#[test]
fn explicit_blank_counts_remain_distinct_from_omitted_axes() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    let config =
        RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation);
    for (text, expected) in [
        ("SUM(TAKE({2,4;8,10},,1))", CellValue::Number(10.)),
        (
            "SUM(TAKE({2,4;8,10},A1,1))",
            CellValue::Error(CellError::Calc),
        ),
        ("SUM(DROP({2,4;8,10},,1))", CellValue::Number(14.)),
        (
            "SUM(DROP({2,4;8,10},A1,1))",
            CellValue::Error(CellError::Calc),
        ),
        ("SUM(EXPAND({2,4;8,10},A1,3,0))", CellValue::Number(24.)),
        (
            "SUM(EXPAND({2,4;8,10},0,3,0))",
            CellValue::Error(CellError::Value),
        ),
        (
            "ISNUMBER(INDEX(EXPAND(2,2,1,A1),2,1))",
            CellValue::Boolean(true),
        ),
    ] {
        let out = run_formula_with_configuration(
            &book,
            0,
            at("Z100").address,
            text,
            &config,
            EvaluationBudgets::default(),
        );
        assert!(
            matches!(&out,EvaluationOutcome::Complete{value,..} if *value==expected),
            "{text}: {out:?}"
        );
    }
}
#[test]
fn arity_error_values_and_documented_grid_limits() {
    // Microsoft's Excel specifications permit 255 function arguments.
    // https://support.microsoft.com/en-us/excel/excel-specifications-and-limits
    for text in [
        "HSTACK()",
        "VSTACK()",
        "TAKE(1)",
        "DROP(1)",
        "CHOOSECOLS(1)",
        "CHOOSEROWS(1)",
        "EXPAND(1)",
        "TAKE(1,1,1,1)",
        "DROP(1,1,1,1)",
        "EXPAND(1,1,1,1,1)",
    ] {
        assert_eq!(formula(text), CellValue::Error(CellError::Value), "{text}");
    }
    for name in ["HSTACK", "VSTACK", "CHOOSECOLS", "CHOOSEROWS"] {
        let text = format!("{name}({})", vec!["1"; 256].join(","));
        assert_eq!(formula(&text), CellValue::Error(CellError::Value), "{name}");
    }
    assert_eq!(
        formula("SUM(HSTACK(#DIV/0!,2))"),
        CellValue::Error(CellError::Div0)
    );
    assert_eq!(
        formula("INDEX(EXPAND(1,2,2,#REF!),2,2)"),
        CellValue::Error(CellError::Ref)
    );
    assert_eq!(formula("INDEX(EXPAND(1,1,16384,0),1,16384)"), 0.0.into());
    assert_eq!(
        formula("EXPAND(1,1,16385,0)"),
        CellValue::Error(CellError::Num)
    );
    assert_eq!(
        formula(&format!("SUM(HSTACK({}))", vec!["1"; 255].join(","))),
        255.0.into()
    );
}
