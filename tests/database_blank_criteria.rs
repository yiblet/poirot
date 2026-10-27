//! Independently authored database criteria checks for blank alternative rows.
use poirot::CellValue;
use poirot::calculation::EvaluationContext;
use poirot::eval::overlay::Evaluation;
use poirot::eval::overlay::Overlay;
use poirot::model::Workbook;

/// Build a three-record database with direct and named criteria formulas.
fn database() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    for (address, value) in [
        ("A1", "Group"),
        ("B1", "Amount"),
        ("E1", "Group"),
        ("E2", ">1"),
        ("F1", "Group"),
    ] {
        sheet.set_value(address, value).unwrap();
    }
    for (address, value) in [
        ("A2", 1.),
        ("A3", 2.),
        ("A4", 3.),
        ("B2", 2.),
        ("B3", 4.),
        ("B4", 6.),
    ] {
        sheet.set_value(address, value).unwrap();
    }
    book.define_name("Criteria", "Data!$E$1:$E$3", None)
        .unwrap();
    book
}

#[test]
fn blank_criteria_rows_are_unrestricted_alternatives_for_direct_and_named_ranges() {
    // Microsoft DVAR remarks: a blank criteria line applies to the entire column.
    // https://support.microsoft.com/en-us/excel/functions/dvar-function
    // This dataset is independently authored, not copied from the source corpus.
    // For 2,4,6: sum=12, sample variance=4, population variance=8/3.
    // Restricting Group>1 selects 4,6: sum=10, variances=2 and1.
    let book = database();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let result = direct.target("Data!H1").unwrap();
    for criteria in ["E1:E2", "E1:E3", "Criteria", "F1:F2"] {
        let unrestricted = criteria != "E1:E2";
        for (function, restricted, all) in [
            ("DVAR", 2., 4.),
            ("DVARP", 1., 8. / 3.),
            ("DSUM", 10., 12.),
            ("DCOUNT", 2., 3.),
        ] {
            let formula = format!("{function}(A1:B4,\"Amount\",{criteria})");
            direct.set_formula(result, &formula).unwrap();
            overlay.set_formula(result, &formula).unwrap();
            let expected = if unrestricted { all } else { restricted };
            for actual in [
                direct.cell(result).unwrap(),
                overlay.evaluation(&context).unwrap().cell(result).unwrap(),
            ] {
                let CellValue::Number(actual) = actual else {
                    panic!("{formula}: {actual:?}")
                };
                assert!(
                    (actual - expected).abs() < 1e-12,
                    "{formula}: {actual} != {expected}"
                );
            }
        }
    }
}

#[test]
fn editing_a_blank_alternative_invalidates_direct_and_overlay_results() {
    let book = database();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let criteria = direct.target("Data!E3").unwrap();
    let result = direct.target("Data!H1").unwrap();
    let formula = "DVAR(A1:B4,\"Amount\",Criteria)";
    direct.set_formula(result, formula).unwrap();
    overlay.set_formula(result, formula).unwrap();
    for (condition, expected) in [
        (CellValue::Blank, 4.),
        (CellValue::Text(">2".into()), 2.),
        (CellValue::Blank, 4.),
        (CellValue::Text("<2".into()), 4.),
    ] {
        direct.set_value(criteria, condition.clone()).unwrap();
        overlay.set_value(criteria, condition).unwrap();
        assert_eq!(direct.cell(result).unwrap(), CellValue::Number(expected));
        assert_eq!(
            overlay.evaluation(&context).unwrap().cell(result).unwrap(),
            CellValue::Number(expected)
        );
    }
}
