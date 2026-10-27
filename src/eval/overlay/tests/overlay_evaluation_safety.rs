use super::super::incremental::Overlay;
use crate::eval::config::EvaluationContext;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::EvaluationError;
use crate::model::{CellAddress, CellValue, Workbook};

#[test]
fn separate_public_calls_preserve_complete_exports_while_reusing_empty_buffers() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet.set_formula("A1", "SEQUENCE(2,2)").unwrap();
    sheet.set_formula("A4", "SEQUENCE(2,2)").unwrap();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    let context = EvaluationContext::default();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    let target = |address: &str| EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    };
    assert_eq!(evaluation.cell(target("B2")).unwrap(), CellValue::from(4.));
    let first = evaluation.result_changes().unwrap();
    let reads = evaluation.session.reuse.overlay_reads.as_ref().unwrap();
    assert!(reads.fresh_results.is_empty());
    assert!(reads.completed.is_empty());
    assert!(reads.fresh_results.capacity() > 0);
    assert!(reads.completed.capacity() > 0);
    assert!(
        evaluation
            .session
            .state
            .runtime
            .preparation
            .publication()
            .unwrap()
            .pending_cells()
            .is_empty()
    );
    assert_eq!(evaluation.cell(target("B5")).unwrap(), CellValue::from(4.));
    let changes = evaluation.result_changes().unwrap();
    assert!(first.iter().all(|change| changes.contains(change)));
    let mut published = workbook.clone();
    published.apply(changes).unwrap();
    let sheet = published.sheet(0).unwrap();
    for (anchor, first_row) in [("A1", 0), ("A4", 3)] {
        assert_eq!(
            sheet.spill_extent(anchor.parse().unwrap()),
            Some(CellAddress {
                row: first_row + 1,
                col: 1
            })
        );
        for row in 0..2 {
            for col in 0..2 {
                assert_eq!(
                    sheet
                        .get(CellAddress {
                            row: first_row + row,
                            col
                        })
                        .unwrap()
                        .unwrap()
                        .value,
                    CellValue::from(f64::from(row * 2 + col + 1))
                );
            }
        }
    }
}

#[test]
fn rejected_cache_commit_keeps_pending_spill_evidence_and_publishes_nothing() {
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("S")
        .unwrap()
        .set_formula("A1", "{1,2}")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    let context = EvaluationContext::default();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    // Constrain retained publication only; private computation remains allowed.
    evaluation.retention.cells = 0;
    let target = EvaluationTarget {
        sheet: 0,
        address: "B1".parse().unwrap(),
    };
    assert!(matches!(
        evaluation.cell(target),
        Err(EvaluationError::MemoRetentionLimit { .. })
    ));
    let publication = evaluation
        .session
        .state
        .runtime
        .preparation
        .publication()
        .unwrap();
    assert_eq!(publication.pending_cells().len(), 2);
    assert_eq!(publication.generated_count(), 2);
    assert_eq!(evaluation.cache.borrow().len(), 0);
    assert!(evaluation.completed.is_empty());
    assert!(evaluation.result_changes().unwrap().is_empty());
    assert!(evaluation.cell(target).is_err());
    assert_eq!(
        evaluation
            .session
            .state
            .runtime
            .preparation
            .publication()
            .unwrap()
            .pending_cells()
            .len(),
        2
    );
}

#[test]
fn conflicting_cache_borrow_returns_stale_plan() {
    let workbook = Workbook::new();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    let context = EvaluationContext::default();
    let evaluation = overlay.evaluation(&context).unwrap();
    let _held = evaluation.cache.borrow_mut();
    assert!(matches!(
        evaluation.result_changes(),
        Err(EvaluationError::StalePlan)
    ));
}
