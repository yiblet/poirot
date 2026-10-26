//! Standard simple folding is shared by public comparison and search consumers.
use poirot::calculation::EvaluationContext;
use poirot::eval::overlay::Evaluation;
use poirot::eval::overlay::Overlay;
use poirot::model::Workbook;
use poirot::{CellError, CellValue};

#[test]
fn simple_folding_is_consistent_across_public_value_comparisons() {
    // Unicode16 CaseFolding C/S mappings; F expansions and Turkic T are excluded.
    // https://www.unicode.org/Public/16.0.0/ucd/CaseFolding.txt
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    sheet.set_value("B2".parse().unwrap(), 9.).unwrap();
    sheet.set_value("B3".parse().unwrap(), "Σ").unwrap();
    sheet.set_value("C3".parse().unwrap(), "ẞ").unwrap();
    sheet.set_value("C4".parse().unwrap(), 9.).unwrap();
    sheet.set_value("E3".parse().unwrap(), "ς").unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let target = direct.target("Data!H1").unwrap();
    for (left, right, equal) in [
        ("I", "i", true),
        ("İ", "i", false),
        ("ı", "i", false),
        ("Σ", "ς", true),
        ("ß", "ẞ", true),
        ("ß", "ss", false),
        ("\u{10400}", "\u{10428}", true),
    ] {
        for (cell, value) in [("A1", left), ("B1", right), ("B4", right), ("E4", left)] {
            let at = direct.target(&format!("Data!{cell}")).unwrap();
            direct.set_value(at, value).unwrap();
            overlay.set_value(at, value).unwrap();
        }
        for (formula, expected) in [
            ("A1=B1", CellValue::Boolean(equal)),
            (
                "MATCH(A1,B1:B1,0)",
                if equal {
                    CellValue::Number(1.)
                } else {
                    CellValue::Error(CellError::NA)
                },
            ),
            (
                "XMATCH(A1,B1:B1)",
                if equal {
                    CellValue::Number(1.)
                } else {
                    CellValue::Error(CellError::NA)
                },
            ),
            (
                "XLOOKUP(A1,B1:B1,B2:B2)",
                if equal {
                    CellValue::Number(9.)
                } else {
                    CellValue::Error(CellError::NA)
                },
            ),
            (
                "COUNTIF(B1:B1,A1)",
                CellValue::Number(if equal { 1. } else { 0. }),
            ),
            (
                "COUNTIF(B1:B1,A1&\"*\")",
                CellValue::Number(if equal { 1. } else { 0. }),
            ),
            (
                "DSUM(B3:C4,\"ß\",E3:E4)",
                CellValue::Number(if equal { 9. } else { 0. }),
            ),
        ] {
            direct.set_formula(target, formula).unwrap();
            overlay.set_formula(target, formula).unwrap();
            assert_eq!(
                direct.cell(target).unwrap(),
                expected,
                "direct {left}/{right}: {formula}"
            );
            assert_eq!(
                overlay.evaluation(&context).unwrap().cell(target).unwrap(),
                expected,
                "overlay {left}/{right}: {formula}"
            );
        }
    }
}

#[test]
fn comparisons_preserve_utf16_units_and_use_scalar_order_without_length_ties() {
    let mut book = Workbook::new();
    book.add_sheet("Data").unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let left = direct.target("Data!A1").unwrap();
    let right = direct.target("Data!B1").unwrap();
    let result = direct.target("Data!H1").unwrap();
    for (a, b, equal, less) in [
        (vec![0xd800], vec![0xd800], true, false),
        (vec![0xd800], vec![0xfffd], false, true),
        (vec![0xdc00, 65], vec![0xdc00, 97], true, false),
        (
            "\u{10400}".encode_utf16().collect(),
            vec![0xe000],
            false,
            false,
        ),
    ] {
        let a = CellValue::from(poirot::model::ExcelText::from_units(a));
        let b = CellValue::from(poirot::model::ExcelText::from_units(b));
        direct.set_value(left, a.clone()).unwrap();
        direct.set_value(right, b.clone()).unwrap();
        overlay.set_value(left, a).unwrap();
        overlay.set_value(right, b).unwrap();
        for (formula, expected) in [("A1=B1", equal), ("A1<B1", less)] {
            direct.set_formula(result, formula).unwrap();
            overlay.set_formula(result, formula).unwrap();
            assert_eq!(direct.cell(result).unwrap(), CellValue::Boolean(expected));
            assert_eq!(
                overlay.evaluation(&context).unwrap().cell(result).unwrap(),
                CellValue::Boolean(expected)
            );
        }
        direct.set_formula(result, "MATCH(A1,A1:A1,0)").unwrap();
        assert_eq!(direct.cell(result).unwrap(), CellValue::Number(1.));
    }
}

#[test]
fn explicit_comparison_limits_remain_operational() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    sheet.set_value("A1".parse().unwrap(), "ΣΣΣ").unwrap();
    sheet.set_value("B1".parse().unwrap(), "ςςς").unwrap();
    sheet
        .set_formula("C1".parse().unwrap(), "IFERROR(A1=B1,7)")
        .unwrap();
    let context = EvaluationContext {
        limits: (poirot::eval::EvaluationBudgets {
            text_bytes: 2,
            ..Default::default()
        })
        .into(),
        ..Default::default()
    };
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let target = direct.target("Data!C1").unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    assert!(matches!(
        direct.cell(target),
        Err(poirot::eval::EvaluationFailure::Budget(_))
    ));
    assert!(matches!(
        overlay.evaluation(&context).unwrap().cell(target),
        Err(poirot::eval::EvaluationFailure::Budget(_))
    ));
}
