//! Discovery performance and lifecycle through the ordinary editable public APIs.
//! Expected arithmetic follows Microsoft's SUM contract:
//! https://support.microsoft.com/en-us/excel/functions/sum-function
//! COUNT includes only numeric members of references:
//! https://support.microsoft.com/en-us/excel/functions/count-function
//! Array coordinates follow documented spilling into adjacent cells:
//! https://support.microsoft.com/en-us/excel/dynamic-array-formulas-and-spilled-array-behavior
//! Work counters and complete per-sheet preparation are Poirot contracts.
use crate::eval::config::EvaluationContext;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::overlay::{Evaluation, Overlay};
use crate::model::{CellValue, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

#[test]
fn disjoint_ranges_do_not_reinspect_preceding_literals_or_scalar_formulas() {
    for formulas in [false, true] {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        for row in 1..=256 {
            let address = at(&format!("A{row}")).address;
            if formulas {
                sheet.set_formula(address, "1*1").unwrap();
            } else {
                sheet.set_value(address, 1.).unwrap();
            }
        }
        for row in 1..=64 {
            sheet
                .set_formula(
                    at(&format!("B{row}")).address,
                    format!("SUM(A{}:A{})", row * 4 - 3, row * 4),
                )
                .unwrap();
        }
        let mut direct = Evaluation::with_workbook(&book)
            .build(EvaluationContext::default())
            .unwrap();
        let mut overlay = Overlay::with_workbook(&book).build().unwrap();
        let context = EvaluationContext::default();
        let mut incremental = overlay.evaluation(&context).unwrap();
        for row in 1..=64 {
            let target = at(&format!("B{row}"));
            assert_eq!(direct.cell(target).unwrap(), CellValue::Number(4.));
            assert_eq!(incremental.cell(target).unwrap(), CellValue::Number(4.));
        }
        // A generous linear bound admits implementation changes while preventing
        // quadratic rescans of preceding source cells, including scalar formulas.
        assert!(direct.finish().outcome.usage().reference_pairs <= 512);
        assert!(incremental.finish().outcome.usage().reference_pairs <= 512);
    }
}

#[test]
fn disjoint_prefixes_leave_irrelevant_sources_uncompiled_until_demanded() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_formula(at("M5000").address, "unresolved_name")
        .unwrap();
    sheet.set_formula(at("X2").address, "{1;2}").unwrap();
    let mut direct = Evaluation::with_workbook(&book)
        .build(EvaluationContext::default())
        .unwrap();
    for address in ["A10000", "ZZ1", "B9000", "Z1"] {
        assert_eq!(direct.cell(at(address)).unwrap(), CellValue::Blank);
    }
    // Each demand excludes both sources: the tall prefixes stop before M,
    // while the wide prefixes stop before row 2. Their union excludes both.
    assert_eq!(direct.compiled_usage().parse_attempts, 0);
    // Reaching each source later compiles it once, preserving its own result.
    assert_eq!(direct.cell(at("X3")).unwrap(), CellValue::Number(2.));
    assert_eq!(direct.compiled_usage().parse_attempts, 1);
    assert_eq!(direct.cell(at("X2")).unwrap(), CellValue::Number(1.));
    assert_eq!(direct.cell(at("M5001")).unwrap(), CellValue::Blank);
    assert_eq!(direct.compiled_usage().parse_attempts, 2);
    assert_eq!(
        direct.cell(at("M5000")).unwrap(),
        CellValue::Error(crate::model::CellError::Name)
    );

    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    let mut incremental = overlay.evaluation(&context).unwrap();
    for address in ["A10000", "ZZ1", "B9000", "Z1"] {
        assert_eq!(incremental.cell(at(address)).unwrap(), CellValue::Blank);
    }
    assert_eq!(incremental.cell(at("X3")).unwrap(), CellValue::Number(2.));
    assert_eq!(incremental.cell(at("M5001")).unwrap(), CellValue::Blank);
}

#[test]
fn discovery_guards_survive_producer_insertion_replacement_and_removal() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    for formula in [None, Some("{1,2}"), Some("{3;4}"), None, Some("{7,8}")] {
        match formula {
            Some(formula) => {
                direct.set_formula(at("G1"), formula).unwrap();
                overlay.set_formula(at("G1"), formula).unwrap();
            }
            None => {
                direct.remove_cell(at("G1")).unwrap();
                overlay.remove_cell(at("G1")).unwrap();
            }
        }
        let (horizontal, vertical) = match formula {
            Some("{1,2}") => (CellValue::Number(2.), CellValue::Blank),
            Some("{3;4}") => (CellValue::Blank, CellValue::Number(4.)),
            Some("{7,8}") => (CellValue::Number(8.), CellValue::Blank),
            _ => (CellValue::Blank, CellValue::Blank),
        };
        let mut incremental = overlay.evaluation(&context).unwrap();
        for (address, expected) in [("H1", horizontal), ("G2", vertical)] {
            assert_eq!(direct.cell(at(address)).unwrap(), expected);
            assert_eq!(incremental.cell(at(address)).unwrap(), expected);
        }
    }
}

#[test]
fn source_only_rectangles_preserve_order_and_reference_admission_after_edits() {
    // Range traversal order is an evaluator contract. These magnitudes expose
    // reassociation: row-major (((0 + 1e16) + 1) - 1e16) + 2 is exactly 2.
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (address, value) in [("A1", 1e16), ("B1", 1.), ("A2", -1e16), ("B2", 2.)] {
        sheet.set_value(at(address).address, value).unwrap();
    }
    sheet.set_formula(at("D1").address, "SUM(A1:B2)").unwrap();
    sheet.set_formula(at("D2").address, "COUNT(A1:B2)").unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    for step in 0..3 {
        if step == 1 {
            // Tombstones and replacement coordinates must retain row-major order.
            direct.remove_cell(at("B1")).unwrap();
            overlay.remove_cell(at("B1")).unwrap();
            direct.set_value(at("B2"), 5.).unwrap();
            overlay.set_value(at("B2"), 5.).unwrap();
        }
        if step == 2 {
            // SUM and COUNT ignore referenced text and logical values.
            direct.set_value(at("B1"), "ignored").unwrap();
            overlay.set_value(at("B1"), "ignored").unwrap();
            direct.set_value(at("B2"), true).unwrap();
            overlay.set_value(at("B2"), true).unwrap();
        }
        let mut incremental = overlay.evaluation(&context).unwrap();
        let (sum, count) = [(2., 4.), (5., 3.), (0., 2.)][step];
        for (address, expected) in [("D1", sum), ("D2", count)] {
            assert_eq!(
                direct.cell(at(address)).unwrap(),
                CellValue::Number(expected)
            );
            assert_eq!(
                incremental.cell(at(address)).unwrap(),
                CellValue::Number(expected)
            );
        }
    }
}

#[test]
fn source_only_and_spilled_rectangles_share_membership_and_collection_limits() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula(at("D1").address, "SUM(A1:B2)").unwrap();
    sheet.set_formula(at("D2").address, "COUNT(A1:B2)").unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    for producer in [None, Some("{1,2;3,4}"), Some("{5,6;7,8}"), None] {
        if let Some(formula) = producer {
            direct.set_formula(at("A1"), formula).unwrap();
            overlay.set_formula(at("A1"), formula).unwrap();
        } else {
            direct.remove_cell(at("A1")).unwrap();
            overlay.remove_cell(at("A1")).unwrap();
        }
        let expected = match producer {
            None => (0., 0.),
            Some("{1,2;3,4}") => (10., 4.),
            Some(_) => (26., 4.),
        };
        let mut incremental = overlay.evaluation(&context).unwrap();
        for (address, value) in [("D1", expected.0), ("D2", expected.1)] {
            assert_eq!(direct.cell(at(address)).unwrap(), CellValue::Number(value));
            assert_eq!(
                incremental.cell(at(address)).unwrap(),
                CellValue::Number(value)
            );
        }
    }
    // Skipping a merge cannot skip source collection admission or its failure.
    let mut limited_book = Workbook::new();
    let sheet = limited_book.add_sheet("S").unwrap();
    for address in ["A1", "B1", "A2", "B2"] {
        sheet.set_value(at(address).address, 1.).unwrap();
    }
    sheet.set_formula(at("D1").address, "SUM(A1:B2)").unwrap();
    let mut limited = context;
    limited.limits = limited.limits.max_collection_values(3);
    let mut direct = Evaluation::with_workbook(&limited_book)
        .build(limited.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&limited_book).build().unwrap();
    assert!(
        matches!(direct.cell(at("D1")), Err(crate::eval::limits::EvaluationError::Budget(failure)) if failure.kind == crate::eval::limits::BudgetKind::Materialization)
    );
    assert!(
        matches!(overlay.evaluation(&limited).unwrap().cell(at("D1")), Err(crate::eval::limits::EvaluationError::Budget(failure)) if failure.kind == crate::eval::limits::BudgetKind::Materialization)
    );
}
