//! Hand-derived cross tabs using Microsoft's PIVOTBY argument and totals contract.
//! https://support.microsoft.com/en-us/excel/functions/pivotby-function
//! The support page's first illustrated example shows the blank corner, column keys
//! and totals used to derive the layout expectations.
//! Data and arithmetic below are independently authored, not copied fixtures or algorithms.
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::eval::*;
use poirot::model::{CellError, CellValue, Workbook};

fn target(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}

fn result(book: &Workbook, formula: &str) -> CellValue {
    match run_formula_with_configuration(
        book,
        0,
        target("Z100").address,
        formula,
        &RunConfiguration::default(),
        EvaluationBudgets::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{formula}: {other:?}"),
    }
}

fn formula(source: &str) -> CellValue {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    result(&book, source)
}

#[test]
fn two_axes_recompute_grand_totals_and_preserve_labels() {
    // Hand-calculated rows: (7,3,10) and (11,7,18); column totals: (18,10,28).
    let base = "PIVOTBY({2;1;1;2;1},{20;10;20;10;10},{7;2;3;11;5},SUM,0)";
    for (row, column, expected) in [
        (1, 2, 10.0),
        (1, 3, 20.0),
        (2, 1, 1.0),
        (2, 2, 7.0),
        (2, 3, 3.0),
        (2, 4, 10.0),
        (3, 2, 11.0),
        (3, 3, 7.0),
        (3, 4, 18.0),
        (4, 2, 18.0),
        (4, 3, 10.0),
        (4, 4, 28.0),
    ] {
        assert_eq!(
            formula(&format!("INDEX({base},{row},{column})")),
            expected.into()
        );
    }
    assert_eq!(
        formula(&format!("INDEX({base},1,4)")),
        CellValue::Text("Total".into())
    );
    assert_eq!(
        formula(&format!("INDEX({base},4,1)")),
        CellValue::Text("Total".into())
    );
    let average = base.replace("SUM", "AVERAGE");
    assert_eq!(
        formula(&format!("INDEX({average},2,4)")),
        (10.0 / 3.0).into()
    );
    assert_eq!(
        formula(&format!("INDEX({average},4,4)")),
        (28.0 / 5.0).into()
    );
}

#[test]
fn source_order_headers_filtering_and_both_axis_sorts() {
    for (call, expected) in [
        (
            r#"INDEX(PIVOTBY({2;1;1;2},{10;10;10;10},{"a";"b";"c";"d"},CONCAT,0),4,2)"#,
            CellValue::Text("abcd".into()),
        ),
        (
            r#"INDEX(PIVOTBY({2;1;1;2},{10;10;10;10},{"a";"b";"c";"d"},CONCAT,0,0),2,2)"#,
            CellValue::Text("bc".into()),
        ),
        (
            r#"INDEX(PIVOTBY({"Row";1;2},{"Column";10;10},{"Value";5;8},SUM,1,0),2,2)"#,
            5.0.into(),
        ),
        (
            r#"INDEX(PIVOTBY({"Row";1;2},{"Column";10;10},{"Value";5;8},SUM,,0),2,2)"#,
            5.0.into(),
        ),
        (
            "INDEX(PIVOTBY({1;2;3},{10;20;10},{#N/A;8;5},SUM,0,0,,0,,{FALSE;TRUE;TRUE}),2,3)",
            8.0.into(),
        ),
        (
            "INDEX(PIVOTBY({1;2},{10;20},{3;7},SUM,0,-1,-1,-1,-1),1,2)",
            CellValue::Text("Total".into()),
        ),
        (
            "INDEX(PIVOTBY({1;2},{10;20},{3;7},SUM,0,0,-2,0,-2),2,1)",
            2.0.into(),
        ),
        (
            "INDEX(PIVOTBY({1;2},{10;20},{3;7},SUM,0,0,-2,0,-2),1,2)",
            20.0.into(),
        ),
        ("IF(FALSE,PIVOTBY(1,2,3,MissingReducer),17)", 17.0.into()),
        (
            "INDEX(PIVOTBY({1;1},{10;10},{#N/A;7},SUM,0,0,,0),2,2)",
            CellValue::Error(CellError::NA),
        ),
        (
            "INDEX(PIVOTBY({1;1;1},{10;10;10},{2;#N/A;TRUE},COUNT,0,0,,0),2,2)",
            1.0.into(),
        ),
    ] {
        assert_eq!(formula(call), expected, "{call}");
    }
}

#[test]
fn missing_crossings_are_empty_and_not_empty_set_reductions() {
    // First-hand Microsoft-hosted SUM workbook report (not a universal callable contract):
    // https://learn.microsoft.com/en-au/answers/questions/4454907/pivotby-function-suggestion-option-to-show-empty-c
    // It reports empty cross-tab cells that cannot be filled from inside the reducer.
    assert_eq!(
        formula("LEN(INDEX(PIVOTBY({1;2},{10;20},{3;7},SUM,0,0,,0),2,3))"),
        0.0.into()
    );
    assert_eq!(
        formula("INDEX(PIVOTBY({1;2},{10;20},{0;7},SUM,0,0,,0),2,2)"),
        0.0.into()
    );
}

#[test]
fn multilevel_both_axes_subtotals_and_relative_sets() {
    let arguments = "{1,1;1,1;1,2;1,2;2,1;2,1},{10,1;10,2;10,1;20,1;10,1;20,1},{2;3;5;7;11;13}";
    // Depth 2 orders each axis as leaves (1,1), (1,2), subtotal 1,
    // leaf (2,1), subtotal 2, then the grand total.
    let sum = format!("PIVOTBY({arguments},SUM,0,2,,2)");
    assert_eq!(formula(&format!("INDEX({sum},5,3)")), 7.0.into());
    assert_eq!(formula(&format!("INDEX({sum},8,8)")), 41.0.into());
    // At row (1,1), column (10,1), the subset sums to 2. Denominators are
    // column 18, row 5, grand total 41, parent column 5 and parent row 7.
    for (relative, expected) in [
        (0, 2.0 / 18.0),
        (1, 2.0 / 5.0),
        (2, 2.0 / 41.0),
        (3, 2.0 / 5.0),
        (4, 2.0 / 7.0),
    ] {
        let call = format!("INDEX(PIVOTBY({arguments},PERCENTOF,0,0,,0,,,{relative}),3,3)");
        assert_eq!(formula(&call), expected.into(), "{call}");
    }
    let reverse = format!("PIVOTBY({arguments},SUM,0,-2,{{1,-2}},-2,{{1,-2}})");
    assert_eq!(formula(&format!("INDEX({reverse},3,3)")), 41.0.into());
}

#[test]
fn invalid_shapes_controls_and_arity_remain_spreadsheet_errors() {
    for call in [
        "PIVOTBY(1,2,3)",
        "PIVOTBY(1,2,3,SUM,4)",
        "PIVOTBY({1;2},1,{3;4},SUM,0)",
        "PIVOTBY({1;2},{1;2},{3;4;5},SUM,0)",
        "PIVOTBY(1,2,3,SUM,0,2)",
        "PIVOTBY(1,2,3,SUM,0,0,0)",
        "PIVOTBY(1,2,3,SUM,0,0,,2)",
        "PIVOTBY(1,2,3,SUM,0,0,,0,0)",
        "PIVOTBY({1;2},{1;2},{3;4},SUM,0,0,,0,,{TRUE,FALSE})",
        "PIVOTBY(1,2,3,SUM,0,0,,0,,,5)",
    ] {
        assert_eq!(formula(call), CellValue::Error(CellError::Value), "{call}");
    }
    assert_eq!(
        formula("PIVOTBY(1,2,3,SUM,0,0,,0,,FALSE)"),
        CellValue::Error(CellError::Calc)
    );
    assert_eq!(
        formula("INDEX(PIVOTBY(1,2,0,PERCENTOF,0,0,,0),2,2)"),
        CellValue::Error(CellError::Div0)
    );
}

#[test]
fn valid_unimplemented_callable_and_layout_modes_are_operational() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    for source in [
        "PIVOTBY(1,2,3,LAMBDA(x,SUM(x)),0)",
        "PIVOTBY(1,2,{3,4},SUM,0)",
        "PIVOTBY(1,2,3,HSTACK(SUM,AVERAGE),0)",
        "PIVOTBY(1,2,3,SUM,2)",
        r#"PIVOTBY({"Row";1},{"Column";2},{"Value";3},SUM,3)"#,
        "PIVOTBY({1,2},{3,4},5,SUM)",
    ] {
        assert!(
            matches!(
                run_formula_with_configuration(
                    &book,
                    0,
                    target("Z100").address,
                    &format!("IFERROR({source},99)"),
                    &RunConfiguration::default(),
                    EvaluationBudgets::default()
                ),
                EvaluationOutcome::Aborted {
                    failure: EvaluationError::UnsupportedCapability { .. },
                    ..
                }
            ),
            "{source}"
        );
    }
}

#[test]
fn direct_overlay_source_changes_and_spill_geometry() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, value) in [
        ("A1", 1.0),
        ("A2", 1.0),
        ("A3", 2.0),
        ("B1", 10.0),
        ("B2", 20.0),
        ("B3", 10.0),
        ("C1", 2.0),
        ("C2", 8.0),
        ("C3", 6.0),
        ("D1", 1.0),
        ("D2", 1.0),
        ("D3", 1.0),
    ] {
        sheet.set_value(cell, value).unwrap();
    }
    sheet
        .set_formula("F1", "PIVOTBY(A1:A3,B1:B3,C1:C3,SUM,0,0,,0,,D1:D3)")
        .unwrap();
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(direct.cell(target("G2")).unwrap(), 2.0.into());
    assert_eq!(direct.cell(target("H2")).unwrap(), 8.0.into());
    drop(direct);
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("G3"))
            .unwrap(),
        6.0.into()
    );
    overlay.set_value(target("C1"), 3.0).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("G2"))
            .unwrap(),
        3.0.into()
    );
    overlay.set_value(target("D2"), 0.0).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("H2"))
            .unwrap(),
        CellValue::Blank
    );
    overlay.set_value(target("A3"), 1.0).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("G2"))
            .unwrap(),
        9.0.into()
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("G3"))
            .unwrap(),
        CellValue::Blank
    );
}

#[test]
fn budget_refusals_and_blocked_spills_preserve_operational_policy() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    assert!(matches!(
        run_formula_with_configuration(
            &book,
            0,
            target("Z100").address,
            "IFERROR(SUM(PIVOTBY(SEQUENCE(100),SEQUENCE(100),SEQUENCE(100),SUM,0,0,,0)),99)",
            &RunConfiguration::default(),
            EvaluationBudgets {
                materialized_values: 1000,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
    let sheet = book.sheet_mut(0).unwrap();
    sheet
        .set_formula("A1", "PIVOTBY({1;2},{10;20},{3;4},SUM,0,0,,0)")
        .unwrap();
    sheet.set_value("B2", 19.0).unwrap();
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(
        direct.cell(target("A1")).unwrap(),
        CellValue::Error(CellError::Spill)
    );
}

#[test]
fn pivot_layout_and_reducer_rejections_are_typed() {
    use poirot::eval::EvaluationReasonError;
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    for (formula, category) in [
        ("PIVOTBY(1,2,3,SUM,2)", 0),
        ("PIVOTBY(1,2,{3,4},SUM,0)", 1),
        ("PIVOTBY(1,2,3,1,0)", 2),
    ] {
        let outcome = run_formula_with_configuration(
            &book,
            0,
            target("Z100").address,
            formula,
            &RunConfiguration::default(),
            EvaluationBudgets::default(),
        );
        let EvaluationOutcome::Aborted {
            failure: EvaluationError::UnsupportedCapability { reason, .. },
            ..
        } = outcome
        else {
            panic!("expected typed refusal: {outcome:?}")
        };
        assert!(matches!(
            (category, reason),
            (0, EvaluationReasonError::PivotFieldHeaderPublication)
                | (1, EvaluationReasonError::PivotMultipleValuesOrReducers)
                | (2, EvaluationReasonError::PivotCallableReducer)
        ));
    }
}
