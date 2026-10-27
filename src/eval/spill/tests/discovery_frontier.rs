//! Independent down/right geometry proofs; overlap priority is Poirot's policy.
use crate::eval::config::EvaluationContext;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::overlay::{Evaluation, Overlay};
use crate::model::{CellError, CellValue, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

fn book(formulas: &[(&str, &str)]) -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (address, formula) in formulas {
        sheet.set_formula(at(address).address, *formula).unwrap();
    }
    book
}

fn verify(book: &Workbook, expected: &[(&str, CellValue)]) {
    for reverse in [false, true] {
        let mut expected = expected.to_vec();
        if reverse {
            expected.reverse();
        }
        let mut direct = Evaluation::with_workbook(book)
            .build(Default::default())
            .unwrap();
        let mut overlay = Overlay::with_workbook(book).build().unwrap();
        let context = EvaluationContext::default();
        for _ in 0..2 {
            let mut run = overlay.evaluation(&context).unwrap();
            for (address, value) in &expected {
                assert_eq!(
                    direct.cell(at(address)).unwrap(),
                    *value,
                    "direct {address}"
                );
                assert_eq!(run.cell(at(address)).unwrap(), *value, "overlay {address}");
            }
        }
    }
}

#[test]
fn competitor_outside_direct_corner_blocks_the_whole_later_spill() {
    verify(
        &book(&[("B1", "SEQUENCE(100)"), ("A100", "SEQUENCE(1,3)")]),
        &[
            ("A100", CellValue::Error(CellError::Spill)),
            ("B100", 100.0.into()),
            ("C100", CellValue::Blank),
        ],
    );
}

#[test]
fn transitive_competitor_prefixes_expand_right_without_publishing_losers() {
    // H1 beats D2 at H100. D2 loses completely, so A100 can then publish A:E.
    verify(
        &book(&[
            ("H1", "SEQUENCE(100)"),
            ("D2", "SEQUENCE(99,5)"),
            ("A100", "SEQUENCE(1,5)"),
        ]),
        &[
            ("A100", 1.0.into()),
            ("E100", 5.0.into()),
            ("D2", CellValue::Error(CellError::Spill)),
            ("H100", 100.0.into()),
        ],
    );
}

#[test]
fn later_discovery_preserves_settled_values_and_handles_unknown_width() {
    let mut workbook = book(&[
        ("A1", "SEQUENCE(2)"),
        ("F9", "SEQUENCE(1,J1)"),
        ("K20", "SEQUENCE(2)"),
    ]);
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(at("J1").address, 3.)
        .unwrap();
    verify(
        &workbook,
        &[
            ("A2", 2.0.into()),
            ("H9", 3.0.into()),
            ("K21", 2.0.into()),
            ("A1", 1.0.into()),
        ],
    );
}

#[test]
fn derived_frontier_relearns_after_snapshot_and_new_source_outside_old_prefix() {
    let workbook = book(&[("A1", "SEQUENCE(2)")]);
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("A2"))
            .unwrap(),
        2.0.into()
    );
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (baseline, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach(&baseline).unwrap();
    restored.set_formula(at("D4"), "SEQUENCE(2,3)").unwrap();
    let mut run = restored.evaluation(&context).unwrap();
    assert_eq!(run.cell(at("F5")).unwrap(), 6.0.into());
    assert_eq!(run.cell(at("A2")).unwrap(), 2.0.into());
}

#[test]
fn inserting_an_earlier_competitor_outside_the_old_demand_invalidates_the_loser() {
    let workbook = book(&[("A100", "SEQUENCE(1,3)")]);
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&workbook)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    assert_eq!(direct.cell(at("A100")).unwrap(), 1.0.into());
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("A100"))
            .unwrap(),
        1.0.into()
    );
    direct.set_formula(at("B1"), "SEQUENCE(100)").unwrap();
    overlay.set_formula(at("B1"), "SEQUENCE(100)").unwrap();
    assert!(overlay.cached_value(at("A100")).is_none());
    assert_eq!(
        direct.cell(at("A100")).unwrap(),
        CellValue::Error(CellError::Spill)
    );
    let mut run = overlay.evaluation(&context).unwrap();
    assert_eq!(
        run.cell(at("A100")).unwrap(),
        CellValue::Error(CellError::Spill)
    );
    assert_eq!(run.cell(at("B100")).unwrap(), 100.0.into());
}

#[test]
fn prepared_range_members_relearn_after_input_source_and_snapshot_changes() {
    // SEQUENCE's row-major values and SUM's arithmetic give 99, then 21.
    // Poirot's authored-blocker policy then rejects A1 and leaves B3:C4 = 1..4.
    let mut workbook = book(&[("A1", "SEQUENCE(H1,H2)"), ("F1", "SUM(B2:D4)")]);
    let sheet = workbook.sheet_mut(0).unwrap();
    sheet.set_value(at("H1").address, 4.).unwrap();
    sheet.set_value(at("H2").address, 4.).unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&workbook)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    assert_eq!(direct.cell(at("F1")).unwrap(), 99.0.into());
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("F1"))
            .unwrap(),
        99.0.into()
    );
    direct.set_value(at("H1"), 2.).unwrap();
    overlay.set_value(at("H1"), 2.).unwrap();
    assert_eq!(direct.cell(at("F1")).unwrap(), 21.0.into());
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("F1"))
            .unwrap(),
        21.0.into()
    );
    direct.set_formula(at("B3"), "SEQUENCE(2,2)").unwrap();
    overlay.set_formula(at("B3"), "SEQUENCE(2,2)").unwrap();
    // The two-row A1 footprint does not reach B3: keep row 2's 21 plus 10.
    assert_eq!(direct.cell(at("F1")).unwrap(), 31.0.into());
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("F1"))
            .unwrap(),
        31.0.into()
    );
    direct.set_value(at("H1"), 4.).unwrap();
    overlay.set_value(at("H1"), 4.).unwrap();
    assert_eq!(direct.cell(at("F1")).unwrap(), 10.0.into());
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("F1"))
            .unwrap(),
        10.0.into()
    );
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (baseline, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach(&baseline).unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("F1"))
            .unwrap(),
        10.0.into()
    );
}
