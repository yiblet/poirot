//! Exact first-match and wildcard expectations follow Microsoft's MATCH/VLOOKUP documentation:
//! https://support.microsoft.com/en-us/office/match-function-e8dffd45-c762-47d6-bf89-533f4a37673a
//! https://support.microsoft.com/en-us/office/vlookup-function-0bbc8083-26fe-4963-8ab8-93a18ad188a1
//! Unicode simple folding, typed blank/error distinctions and reuse lifetime are Poirot contracts.
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::eval::*;
use poirot::model::{CellError, CellValue, ExcelText, Workbook};

fn at(sheet: usize, address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet,
        address: address.parse().unwrap(),
    }
}
fn set(book: &mut Workbook, sheet: usize, address: &str, value: impl Into<CellValue>) {
    book.sheet_mut(sheet)
        .unwrap()
        .set_value(at(sheet, address).address, value)
        .unwrap();
}
fn formula(book: &mut Workbook, sheet: usize, address: &str, formula: &str) {
    book.sheet_mut(sheet)
        .unwrap()
        .set_formula(at(sheet, address).address, formula)
        .unwrap();
}
fn book() -> Workbook {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    book
}

#[test]
fn ready_numeric_boolean_and_generated_axes_reuse_first_positions() {
    let mut book = book();
    formula(&mut book, 0, "A1", "{1,10;2,20;2,30;FALSE,40}");
    for (address, source) in [
        ("D1", "VLOOKUP(2,A1:B4,2,FALSE)"),
        ("D2", "MATCH(2,A1:A4,0)"),
        ("D3", "VLOOKUP(FALSE,A1:B4,2,FALSE)"),
        ("D4", "MATCH(FALSE,A1:A4,0)"),
    ] {
        formula(&mut book, 0, address, source);
    }
    let mut run = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    for (address, value) in [("D1", 20.), ("D2", 2.), ("D3", 40.), ("D4", 4.)] {
        assert_eq!(run.cell(at(0, address)).unwrap(), CellValue::Number(value));
    }
    assert_eq!(run.usage().lookup_prefix_scans, 4);
    assert_eq!(run.usage().lookup_prefix_hits, 2);
}

#[test]
fn literal_text_identity_keeps_wildcards_unicode_errors_and_duplicate_order() {
    let mut book = book();
    for (address, value) in [
        ("A1", CellValue::Blank),
        ("A2", CellValue::Error(CellError::NA)),
        ("A3", "Key".into()),
        ("A4", "KEY".into()),
        ("A5", "a*b".into()),
        ("A6", "abc".into()),
        ("A7", "".into()),
        ("B3", 30.0.into()),
        ("C3", 300.0.into()),
        (
            "A8",
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800])),
        ),
        (
            "F1",
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800])),
        ),
        ("F2", CellValue::TextUtf16(ExcelText::from("key"))),
    ] {
        set(&mut book, 0, address, value);
    }
    for (address, source) in [
        ("H1", "VLOOKUP(\"key\",A1:B8,2,FALSE)"),
        ("H2", "VLOOKUP(F2,A1:C8,3,FALSE)"),
        ("H3", "MATCH(\"a*\",A1:A8,0)"),
        ("H4", "MATCH(\"a*\",A1:A8,0)"),
        ("H5", "MATCH(\"a~*b\",A1:A8,0)"),
        ("H6", "MATCH(\"\",A1:A8,0)"),
        ("H7", "MATCH(F1,A1:A8,0)"),
        ("H8", "MATCH(F1,A1:A8,0)"),
    ] {
        formula(&mut book, 0, address, source);
    }
    // Errors are ignored by the exact typed comparator; they conservatively
    // prevent retention, so this run also checks the ordinary fallback answers.
    let mut run = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    for (address, value) in [
        ("H1", 30.),
        ("H2", 300.),
        ("H3", 5.),
        ("H4", 5.),
        ("H5", 5.),
        ("H6", 7.),
        ("H7", 8.),
        ("H8", 8.),
    ] {
        assert_eq!(run.cell(at(0, address)).unwrap(), CellValue::Number(value));
    }
    drop(run);
    set(&mut book, 0, "A2", 9.);
    let mut run = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    for (address, value) in [
        ("H1", 30.),
        ("H2", 300.),
        ("H3", 5.),
        ("H4", 5.),
        ("H5", 5.),
        ("H6", 7.),
        ("H7", 8.),
        ("H8", 8.),
    ] {
        assert_eq!(run.cell(at(0, address)).unwrap(), CellValue::Number(value));
    }
    assert!(run.usage().lookup_prefix_hits >= 3);
}

#[test]
fn cross_sheet_first_query_prepares_actual_axis_and_internal_queries_wait_for_ready() {
    let mut book = book();
    book.add_sheet("Other").unwrap();
    formula(&mut book, 1, "A1", "{\"x\",10;\"y\",20}");
    formula(&mut book, 0, "A1", "{1,2}+MATCH(\"y\",Other!A1:A2,0)");
    formula(&mut book, 0, "D1", "VLOOKUP(\"y\",Other!A1:B2,2,FALSE)");
    formula(&mut book, 0, "D2", "MATCH(\"y\",Other!A1:A2,0)");
    let mut run = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(run.cell(at(0, "B1")).unwrap(), 4.0.into());
    assert_eq!(run.usage().lookup_prefix_hits, 0);
    assert_eq!(run.usage().lookup_prefix_scans, 0);
    assert_eq!(run.cell(at(0, "D1")).unwrap(), 20.0.into());
    assert_eq!(run.cell(at(0, "D2")).unwrap(), 2.0.into());
    assert_eq!(run.usage().lookup_prefix_hits, 1);
    assert_eq!(run.usage().lookup_prefix_scans, 2);
}

#[test]
fn text_selections_survive_return_and_needle_edits_and_snapshot_but_expire_on_keys() {
    let mut book = book();
    formula(&mut book, 0, "A1", "{\"first\",10;\"last\",20}");
    set(&mut book, 0, "G1", "last");
    set(&mut book, 0, "G2", 0.);
    formula(&mut book, 0, "D1", "VLOOKUP(G1,A1:B2,2,FALSE)+G2");
    formula(&mut book, 0, "D2", "MATCH(G1,A1:A2,0)+G2");
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at(0, "D1")).unwrap(), 20.0.into());
        assert_eq!(run.cell(at(0, "D2")).unwrap(), 2.0.into());
        assert_eq!(run.usage().lookup_prefix_hits, 1);
    }
    overlay.set_value(at(0, "G2"), 1.).unwrap();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at(0, "D1")).unwrap(), 21.0.into());
        assert_eq!(run.cell(at(0, "D2")).unwrap(), 3.0.into());
        assert_eq!(
            run.usage().lookup_prefix_hits,
            2,
            "restored valid spill keeps its axis"
        );
        assert_eq!(run.usage().lookup_prefix_scans, 0);
    }
    let (book, state) = Overlay::from_snapshot(overlay.snapshot().into_owned().unwrap()).unwrap();
    let mut overlay = state.attach(&book).unwrap();
    overlay.set_value(at(0, "G1"), "first").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at(0, "D1"))
            .unwrap(),
        11.0.into()
    );
    overlay
        .set_formula(at(0, "A1"), "{\"last\",99;\"first\",88}")
        .unwrap();
    let mut run = overlay.evaluation(&context).unwrap();
    assert_eq!(run.cell(at(0, "D1")).unwrap(), 89.0.into());
    assert_eq!(run.cell(at(0, "D2")).unwrap(), 3.0.into());
    assert_eq!(run.usage().lookup_prefix_scans, 2);
}

#[test]
fn growth_from_before_axis_invalidates_hit_and_missing_selections_for_all_consumers() {
    let mut book = book();
    set(&mut book, 0, "G1", false);
    formula(
        &mut book,
        0,
        "A1",
        "IF(G1,{\"a\";\"b\";\"c\";\"d\";\"e\";\"new\"},{\"a\"})",
    );
    set(&mut book, 0, "A8", "old");
    for (address, source) in [
        ("D1", "MATCH(\"old\",A5:A10,0)"),
        ("D2", "MATCH(\"old\",A5:A10,0)"),
        ("D3", "IFERROR(MATCH(\"new\",A5:A10,0),99)"),
        ("D4", "IFERROR(MATCH(\"new\",A5:A10,0),99)"),
    ] {
        formula(&mut book, 0, address, source);
    }
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        for (address, value) in [("D1", 4.), ("D2", 4.), ("D3", 99.), ("D4", 99.)] {
            assert_eq!(run.cell(at(0, address)).unwrap(), CellValue::Number(value));
        }
        assert_eq!(run.usage().lookup_prefix_hits, 2);
    }
    overlay.set_value(at(0, "G1"), true).unwrap();
    let mut run = overlay.evaluation(&context).unwrap();
    for (address, value) in [("D1", 4.), ("D2", 4.), ("D3", 2.), ("D4", 2.)] {
        assert_eq!(run.cell(at(0, address)).unwrap(), CellValue::Number(value));
    }
    assert_eq!(run.usage().lookup_prefix_hits, 2);
    assert!(
        run.usage().lookup_prefix_scans > 0,
        "new footprint expires old selections"
    );
    drop(run);
    for (expanded, expected_new) in [(false, 99.), (true, 2.)] {
        overlay.set_value(at(0, "G1"), expanded).unwrap();
        let mut run = overlay.evaluation(&context).unwrap();
        for (address, expected) in [
            ("D1", 4.),
            ("D2", 4.),
            ("D3", expected_new),
            ("D4", expected_new),
        ] {
            assert_eq!(
                run.cell(at(0, address)).unwrap(),
                CellValue::Number(expected)
            );
        }
        assert_eq!(run.usage().lookup_prefix_hits, 2);
        assert!(run.usage().lookup_prefix_scans > 0);
    }
}

#[test]
fn horizontal_text_queries_and_approximate_queries_keep_distinct_policies() {
    let mut book = book();
    for (address, value) in [("A1", "a"), ("B1", "b"), ("C1", "b")] {
        set(&mut book, 0, address, value);
    }
    for (address, value) in [("A2", 10.), ("B2", 20.), ("C2", 30.)] {
        set(&mut book, 0, address, value);
    }
    for (address, source) in [
        ("E1", "HLOOKUP(\"b\",A1:C2,2,FALSE)"),
        ("E2", "MATCH(\"b\",A1:C1,0)"),
        ("E3", "HLOOKUP(\"b\",A1:C2,2,TRUE)"),
        ("E4", "MATCH(\"b\",A1:C1,1)"),
    ] {
        formula(&mut book, 0, address, source);
    }
    let mut run = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    for (address, value) in [("E1", 20.), ("E2", 2.), ("E3", 30.), ("E4", 3.)] {
        assert_eq!(run.cell(at(0, address)).unwrap(), CellValue::Number(value));
    }
    assert_eq!(run.usage().lookup_prefix_hits, 1);
}

#[test]
fn optional_text_query_storage_declines_keys_larger_than_collection_capacity() {
    let mut book = book();
    let key = "k".repeat(100);
    set(&mut book, 0, "A1", key.clone());
    set(&mut book, 0, "G1", key);
    formula(&mut book, 0, "D1", "MATCH(G1,A1:A1,0)");
    formula(&mut book, 0, "D2", "MATCH(G1,A1:A1,0)");
    let mut run = Evaluation::with_workbook(&book)
        .build(EvaluationContext {
            limits: poirot::eval::EvaluationLimits::default().max_collection_values(32),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(run.cell(at(0, "D1")).unwrap(), 1.0.into());
    assert_eq!(run.cell(at(0, "D2")).unwrap(), 1.0.into());
    assert_eq!(run.usage().lookup_prefix_hits, 0);
    assert_eq!(run.usage().lookup_prefix_scans, 0);
}

#[test]
fn ordered_aggregation_reads_generated_members_despite_other_sheet_spills() {
    let mut book = book();
    book.add_sheet("Other").unwrap();
    formula(&mut book, 1, "A1", "{9,10}");
    formula(&mut book, 0, "A1", "{1;2;3}");
    formula(&mut book, 0, "D1", "SUM(A1:A2)");
    formula(&mut book, 0, "D2", "SUM(A1:A3)");
    let mut run = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(run.cell(at(1, "B1")).unwrap(), 10.0.into());
    assert_eq!(run.cell(at(0, "D1")).unwrap(), 3.0.into());
    assert_eq!(run.cell(at(0, "D2")).unwrap(), 6.0.into());
}

#[test]
fn repeated_text_hits_do_not_spend_retained_query_capacity() {
    let mut book = book();
    set(&mut book, 0, "A1", "longkey");
    for row in 1..=20 {
        formula(
            &mut book,
            0,
            &format!("D{row}"),
            "MATCH(\"longkey\",A1:A1,0)",
        );
    }
    let mut run = Evaluation::with_workbook(&book)
        .build(EvaluationContext {
            limits: poirot::eval::EvaluationLimits::default().max_collection_values(32),
            ..Default::default()
        })
        .unwrap();
    for row in 1..=20 {
        assert_eq!(run.cell(at(0, &format!("D{row}"))).unwrap(), 1.0.into());
    }
    assert_eq!(run.usage().lookup_prefix_scans, 1);
    assert_eq!(run.usage().lookup_prefix_hits, 19);
}

#[test]
fn single_candidate_guards_invalidate_and_read_live_return_values() {
    let mut book = book();
    set(&mut book, 0, "A1", "key");
    set(&mut book, 0, "B1", 10.);
    for address in ["D1", "D2"] {
        formula(
            &mut book,
            0,
            address,
            "IFERROR(VLOOKUP(\"key\",A1:B1,2,FALSE),0)",
        );
    }
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        for address in ["D1", "D2"] {
            assert_eq!(run.cell(at(0, address)).unwrap(), 10.0.into());
        }
        assert_eq!(run.usage().lookup_prefix_hits, 1);
    }
    overlay.set_value(at(0, "B1"), 99.).unwrap();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        for address in ["D1", "D2"] {
            assert_eq!(run.cell(at(0, address)).unwrap(), 99.0.into());
        }
        assert_eq!(run.usage().lookup_prefix_hits, 2);
    }
    overlay.set_value(at(0, "A1"), "other").unwrap();
    let mut run = overlay.evaluation(&context).unwrap();
    for address in ["D1", "D2"] {
        assert_eq!(run.cell(at(0, address)).unwrap(), 0.0.into());
    }
    assert_eq!(run.usage().lookup_prefix_hits, 1);
}

#[test]
fn producer_time_aggregate_preserves_operational_failure_and_atomic_cleanup() {
    let mut book = book();
    formula(&mut book, 0, "A1", "1/0");
    formula(&mut book, 0, "A2", "REPT(\"x\",100)");
    formula(&mut book, 0, "D1", "{1,2}");
    formula(&mut book, 0, "Z1", "{1,2}+SUM(A1:A2)");
    let mut run = Evaluation::with_workbook(&book)
        .build(EvaluationContext {
            limits: poirot::eval::EvaluationLimits::default().max_text_bytes(50),
            ..Default::default()
        })
        .unwrap();
    // Poirot buffers a single reference operand before reducing it. Operational
    // failure must escape instead of becoming the earlier spreadsheet error.
    assert!(matches!(
        run.cell(at(0, "Z1")),
        Err(EvaluationError::Budget(BudgetError {
            kind: BudgetKind::TextBytes,
            requested: 200,
            ..
        }))
    ));
    assert_eq!(
        run.values().count(),
        0,
        "failed preparation admits no partial footprint"
    );
}
