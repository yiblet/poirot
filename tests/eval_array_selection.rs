//! Independently authored contracts from Microsoft documentation, retrieved 2026-10-21.
//! https://support.microsoft.com/en-us/excel/functions/filter-function
//! https://support.microsoft.com/en-us/excel/functions/sort-function
//! https://support.microsoft.com/en-us/excel/functions/sortby-function
//! https://support.microsoft.com/en-us/excel/functions/unique-function
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::eval::*;
use poirot::model::{CellError, CellValue, Workbook};
fn at(s: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: s.parse().unwrap(),
    }
}
fn policy() -> EvaluationPolicy {
    EvaluationPolicy {
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
        &RunConfiguration::default(),
        EvaluationBudgets::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}

#[test]
fn filter_orientation_coercion_and_lazy_fallback() {
    for (f, value) in [
        (
            "SUM(FILTER({2,3;5,7;11,13},{TRUE;FALSE;TRUE}))",
            29.0.into(),
        ),
        ("SUM(FILTER({2,3;5,7},{0,2}))", 10.0.into()),
        ("INDEX(FILTER({2,3;5,7},{TRUE;FALSE},1/0),1,2)", 3.0.into()),
        ("FILTER(7,FALSE,19)", 19.0.into()),
        ("INDEX(FILTER({2;3},{0;0},{11,13}),1,2)", 13.0.into()),
        ("FILTER(7,TRUE)", 7.0.into()),
        ("FILTER(7,FALSE,\"\")", CellValue::Text("".into())),
        ("FILTER(7,FALSE,)", 0.0.into()),
    ] {
        assert_eq!(formula(f), value, "{f}");
    }
    for (f, error) in [
        ("FILTER({2;3},{TRUE,FALSE})", CellError::Value),
        (
            "FILTER({2,3;5,7},{TRUE,FALSE;FALSE,TRUE})",
            CellError::Value,
        ),
        ("FILTER({2;3},{FALSE;#N/A},19)", CellError::NA),
        ("FILTER({2;3},{FALSE;\"green\"},19)", CellError::Value),
        ("FILTER({2;3},{FALSE;FALSE})", CellError::Calc),
        ("FILTER({2;3},{0;0},1/0)", CellError::Div0),
        ("FILTER(1)", CellError::Value),
        ("FILTER(1,TRUE,0,7)", CellError::Value),
    ] {
        assert_eq!(formula(f), CellValue::Error(error), "{f}");
    }
}
#[test]
fn sort_internal_and_external_keys_both_axes_and_stable_ties() {
    for (f, n) in [
        ("INDEX(SORT({5,20;2,30;7,10}),2,1)", 5.),
        ("INDEX(SORT({5,20;2,30;7,10},2,-1),1,1)", 2.),
        ("INDEX(SORT({5,2,7;20,30,10},2,-1,TRUE),1,1)", 2.),
        ("INDEX(SORTBY({11;13;17},{3;1;2}),1)", 13.),
        ("INDEX(SORTBY({11,13,17},{3,1,2},-1),1,1)", 11.),
        (
            "INDEX(SORTBY({11;13;17;19},{2;1;2;1},1,{2;1;1;2},-1),1)",
            19.,
        ),
        ("INDEX(SORTBY({11;13;17},{1;1;1}),2)", 13.),
        ("INDEX(SORT({5;2;7},,),2)", 5.),
        ("SUM(SORT(7))", 7.),
    ] {
        assert_eq!(formula(f), n.into(), "{f}");
    }
    for f in [
        "SORT({2;3},0)",
        "SORT({2;3},2)",
        "SORT({2;3},1,0)",
        "SORT({2;3},1,2)",
        "SORT({2;3},1,1.5)",
        "SORTBY({2;3},{1,2})",
        "SORTBY({2;3},{1;2},0)",
        "SORTBY({2,3;5,7},{1,2;3,4})",
        "SORTBY({2,3;5,7},{1;2},1,{1,2},1)",
        "SORT()",
        "SORTBY(1)",
        "SORT(1,1,1,FALSE,0)",
    ] {
        assert_eq!(formula(f), CellValue::Error(CellError::Value), "{f}");
    }
    assert_eq!(
        formula("SORT({2;3},#REF!)"),
        CellValue::Error(CellError::Ref)
    );
    assert_eq!(
        formula("SORTBY({2;3},{1;2},#N/A)"),
        CellValue::Error(CellError::NA)
    );
}
#[test]
fn unique_rows_columns_case_folding_and_exactly_once() {
    for (f, expected) in [
        ("ROWS(UNIQUE({3,8;1,9;3,8;1,4}))", 3.0.into()),
        ("INDEX(UNIQUE({3,8;1,9;3,8;1,4}),2,2)", 9.0.into()),
        ("ROWS(UNIQUE({3,8;1,9;3,8;1,4},FALSE,TRUE))", 2.0.into()),
        ("INDEX(UNIQUE({3,1,3,2;8,9,8,7},TRUE),2,3)", 7.0.into()),
        ("COLUMNS(UNIQUE({3,1,3,2;8,9,8,7},TRUE,TRUE))", 2.0.into()),
        ("ROWS(UNIQUE({\"Ä\";\"ä\";\"green\"}))", 2.0.into()),
        (
            "INDEX(UNIQUE({\"Ä\";\"ä\";\"green\"}),1)",
            CellValue::Text("Ä".into()),
        ),
        ("ROWS(UNIQUE({1;\"1\";TRUE;#N/A;#N/A;\"\"}))", 5.0.into()),
        ("ROWS(UNIQUE({0;-0;1}))", 2.0.into()),
        ("UNIQUE(7)", 7.0.into()),
        (
            "INDEX(UNIQUE({#N/A;#N/A;#REF!}),2)",
            CellValue::Error(CellError::Ref),
        ),
    ] {
        assert_eq!(formula(f), expected, "{f}");
    }
    for (f, error) in [
        ("UNIQUE({2;2},,TRUE)", CellError::Calc),
        ("UNIQUE({2;3},#N/A)", CellError::NA),
        ("UNIQUE()", CellError::Value),
        ("UNIQUE(1,FALSE,FALSE,0)", CellError::Value),
    ] {
        assert_eq!(formula(f), CellValue::Error(error), "{f}");
    }
}
#[test]
fn original_blanks_remain_distinct_during_sort_and_grouping() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value(at("A2").address, 0.).unwrap();
    sheet.set_formula(at("A3").address, "\"\"").unwrap();
    sheet
        .set_value(at("A4").address, CellValue::Boolean(false))
        .unwrap();
    sheet
        .set_value(at("A5").address, CellValue::Error(CellError::NA))
        .unwrap();
    sheet
        .set_formula(at("D1").address, "UNIQUE(A1:A5)")
        .unwrap();
    sheet.set_formula(at("G1").address, "SORT(A1:A5)").unwrap();
    sheet
        .set_formula(at("J1").address, "FILTER(A1:A5,{TRUE;TRUE;TRUE;TRUE;TRUE})")
        .unwrap();
    sheet
        .set_formula(at("M1").address, "SORT(A1:A5,1,-1)")
        .unwrap();
    let mut direct = Evaluation::with_workbook(&book)
        .with_policy(policy())
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let mut run = overlay.evaluation(&context).unwrap();
    for (cell, value) in [
        ("D6", CellValue::Blank),
        ("D5", CellValue::Error(CellError::NA)),
        ("D4", CellValue::Boolean(false)),
        ("D3", CellValue::Text("".into())),
        ("D2", 0.0.into()),
        ("D1", 0.0.into()),
        ("G5", 0.0.into()),
        ("G4", CellValue::Error(CellError::NA)),
        ("G3", CellValue::Boolean(false)),
        ("G2", CellValue::Text("".into())),
        ("G1", 0.0.into()),
        ("J3", CellValue::Text("".into())),
        ("M5", 0.0.into()),
        ("M4", 0.0.into()),
        ("M3", CellValue::Text("".into())),
        ("M2", CellValue::Boolean(false)),
        ("M1", CellValue::Error(CellError::NA)),
    ] {
        assert_eq!(direct.cell(at(cell)).unwrap(), value, "direct {cell}");
        assert_eq!(run.cell(at(cell)).unwrap(), value, "overlay {cell}");
    }
}
#[test]
fn overlay_edits_resize_spills_and_refresh_masks_sort_keys_and_unique_groups() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, n) in [
        ("A1", 3.),
        ("A2", 7.),
        ("A3", 3.),
        ("B1", 1.),
        ("B2", 0.),
        ("B3", 1.),
    ] {
        sheet.set_value(at(cell).address, n).unwrap();
    }
    for (cell, f) in [
        ("D1", "FILTER(A1:A3,B1:B3)"),
        ("G1", "UNIQUE(A1:A3)"),
        ("J1", "SORTBY(A1:A3,B1:B3)"),
    ] {
        sheet.set_formula(at(cell).address, f).unwrap();
    }
    let mut overlay = Overlay::with_workbook(book)
        .with_policy(policy())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at("D3")).unwrap(), CellValue::Blank);
        assert_eq!(run.cell(at("D2")).unwrap(), 3.0.into());
        assert_eq!(run.cell(at("G3")).unwrap(), CellValue::Blank);
        assert_eq!(run.cell(at("J1")).unwrap(), 7.0.into());
    }
    overlay.set_value(at("B2"), 1.).unwrap();
    overlay.set_value(at("A3"), 11.).unwrap();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at("D3")).unwrap(), 11.0.into());
        assert_eq!(run.cell(at("G3")).unwrap(), 11.0.into());
        assert_eq!(run.cell(at("J1")).unwrap(), 3.0.into());
    }
    overlay.set_value(at("B1"), 0.).unwrap();
    overlay.set_value(at("B2"), 0.).unwrap();
    overlay.set_value(at("A3"), 3.).unwrap();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at("D2")).unwrap(), CellValue::Blank);
        assert_eq!(run.cell(at("G3")).unwrap(), CellValue::Blank);
        assert_eq!(run.cell(at("D1")).unwrap(), 3.0.into());
    }
}
#[test]
fn blockers_and_operational_budgets_are_preserved() {
    for f in [
        "FILTER({2;3},{TRUE;TRUE})",
        "SORT({3;2})",
        "SORTBY({3;2},{1;2})",
        "UNIQUE({3;2})",
    ] {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet.set_formula(at("D1").address, f).unwrap();
        sheet.set_value(at("D2").address, 9.).unwrap();
        let mut direct = Evaluation::with_workbook(&book)
            .with_policy(policy())
            .build(Default::default())
            .unwrap();
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(policy())
            .build()
            .unwrap();
        let context = EvaluationContext::default();
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(direct.cell(at("D2")).unwrap(), 9.0.into());
        assert_eq!(run.cell(at("D2")).unwrap(), 9.0.into());
        assert_eq!(
            direct.cell(at("D1")).unwrap(),
            CellValue::Error(CellError::Spill)
        );
        assert_eq!(
            run.cell(at("D1")).unwrap(),
            CellValue::Error(CellError::Spill)
        );
    }
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    assert!(matches!(
        run_formula_with_configuration(
            &book,
            0,
            at("Z100").address,
            "IFERROR(SUM(SORT(SEQUENCE(1000))),99)",
            &RunConfiguration::default(),
            EvaluationBudgets {
                materialized_values: 100,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}
#[test]
fn lossless_utf16_text_and_nested_reference_inputs_survive_selection() {
    use poirot::model::ExcelText;
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    let first = CellValue::TextUtf16(ExcelText::from_units(vec![0xd800, 65]));
    let equivalent = CellValue::TextUtf16(ExcelText::from_units(vec![0xd800, 97]));
    let distinct = CellValue::TextUtf16(ExcelText::from_units(vec![0xd801]));
    for (cell, value) in [
        ("A1", first.clone()),
        ("A2", equivalent),
        ("A3", distinct.clone()),
    ] {
        sheet.set_value(at(cell).address, value).unwrap();
    }
    sheet
        .set_formula(at("D1").address, "UNIQUE(A1:A3)")
        .unwrap();
    sheet
        .set_formula(at("G1").address, "SORT(FILTER(A1:A3,{TRUE;FALSE;TRUE}))")
        .unwrap();
    let mut direct = Evaluation::with_workbook(&book)
        .with_policy(policy())
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let mut run = overlay.evaluation(&context).unwrap();
    for (cell, value) in [
        ("D3", CellValue::Blank),
        ("D2", distinct.clone()),
        ("D1", first.clone()),
        ("G2", distinct),
        ("G1", first),
    ] {
        assert_eq!(direct.cell(at(cell)).unwrap(), value, "direct {cell}");
        assert_eq!(run.cell(at(cell)).unwrap(), value, "overlay {cell}");
    }
}
#[test]
fn filter_fallback_dependencies_follow_mask_changes() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, n) in [
        ("A1", 2.),
        ("A2", 3.),
        ("B1", 0.),
        ("B2", 0.),
        ("F1", 11.),
        ("F2", 13.),
    ] {
        sheet.set_value(at(cell).address, n).unwrap();
    }
    sheet
        .set_formula(at("D1").address, "FILTER(A1:A2,B1:B2,F1:F2)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(book)
        .with_policy(policy())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at("D2")).unwrap(), 13.0.into());
    }
    overlay.set_value(at("F2"), 17.).unwrap();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at("D2")).unwrap(), 17.0.into());
    }
    overlay.set_value(at("B1"), 1.).unwrap();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at("D2")).unwrap(), CellValue::Blank);
        assert_eq!(run.cell(at("D1")).unwrap(), 2.0.into());
    }
    overlay.set_value(at("F1"), 19.).unwrap();
    overlay.set_value(at("B1"), 0.).unwrap();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at("D1")).unwrap(), 19.0.into());
        assert_eq!(run.cell(at("D2")).unwrap(), 17.0.into());
    }
}
