//! Public batching contracts; expected values follow integer arithmetic and SEQUENCE's row count.
use poirot_eval::{
    ArrayResultPolicy, EvaluationTarget,
    overlay::{EvaluationContext, Overlay, OverlayPolicy},
};
use poirot_model::{CellStyle, CellValue, ModelError, Workbook};
use std::{panic::AssertUnwindSafe, sync::Arc};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

fn workbook() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (address, value) in [("A1", 1.), ("A2", 2.), ("A3", 3.)] {
        sheet.set_value(at(address).address, value).unwrap();
    }
    for (address, formula) in [
        ("B1", "SUM(A1:A3)"),
        ("C1", "IF(A1>0,B1,A3)"),
        ("D1", "7*6"),
    ] {
        sheet.set_formula(at(address).address, formula).unwrap();
    }
    book
}

#[test]
fn batching_matches_sequential_edits_and_preserves_unaffected_results() {
    let book = workbook();
    let context = EvaluationContext::default();
    let mut batch = Overlay::with_workbook(&book).build().unwrap();
    let mut sequential = Overlay::with_workbook(&book).build().unwrap();
    let outputs = [at("B1"), at("C1"), at("D1")];
    for overlay in [&mut batch, &mut sequential] {
        overlay
            .evaluation(&context)
            .unwrap()
            .outputs(&outputs)
            .unwrap();
    }
    let unaffected = batch.cached_value(at("D1")).unwrap();
    let style = CellStyle {
        number_format: Some("0.00".into()),
    };
    sequential.set_value(at("A1"), 5.).unwrap();
    sequential.set_value(at("A1"), 8.).unwrap();
    sequential.set_formula(at("A2"), "A1*2").unwrap();
    sequential.set_cached_value(at("A2"), -1.).unwrap();
    sequential.set_style(at("A2"), style.clone()).unwrap();
    sequential.remove_cell(at("A3")).unwrap();
    batch
        .edit(|edits| -> Result<(), ModelError> {
            edits.set_value(at("A1"), 5.)?;
            edits.set_value(at("A1"), 8.)?;
            edits.set_formula(at("A2"), "A1*2")?;
            edits.set_cached_value(at("A2"), -1.)?;
            edits.set_style(at("A2"), style)?;
            edits.remove_cell(at("A3"))
        })
        .unwrap();
    assert!(batch.cached_value(at("B1")).is_none());
    assert!(Arc::ptr_eq(
        &unaffected,
        &batch.cached_value(at("D1")).unwrap()
    ));
    let actual = batch
        .evaluation(&context)
        .unwrap()
        .outputs(&outputs)
        .unwrap();
    let expected = sequential
        .evaluation(&context)
        .unwrap()
        .outputs(&outputs)
        .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(
        actual.iter().map(|o| o.value.clone()).collect::<Vec<_>>(),
        [24., 24., 42.].map(CellValue::Number)
    );
    assert_eq!(batch.retained_usage(), sequential.retained_usage());
    assert_eq!(batch.changes(), sequential.changes());
    let bytes = postcard::to_allocvec(&batch.snapshot()).unwrap();
    let (baseline, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach(baseline).unwrap();
    let mut run = restored.evaluation(&context).unwrap();
    assert_eq!(run.cell(at("C1")).unwrap(), CellValue::Number(24.));
    assert_eq!(run.usage().formula_executions, 0);
}

#[test]
fn empty_and_noop_batches_keep_result_identity() {
    let book = workbook();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("C1"))
        .unwrap();
    let cached = overlay.cached_value(at("C1")).unwrap();
    overlay.edit(|_| ());
    overlay.edit(|edits| {
        edits.set_value(at("A1"), 1.).unwrap();
        edits.remove_cell(at("Z99")).unwrap();
    });
    assert!(
        overlay
            .edit(|edits| edits.set_formula(at("A2"), ""))
            .is_err()
    );
    assert!(Arc::ptr_eq(
        &cached,
        &overlay.cached_value(at("C1")).unwrap()
    ));
}

#[test]
fn errors_keep_successful_writes_and_flush_invalidation() {
    let book = workbook();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    for failure in 0..3 {
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap();
        let result = overlay.edit(|edits| -> Result<(), ModelError> {
            edits.set_value(at("A1"), f64::from(failure) + 10.)?;
            match failure {
                0 => edits.set_value(
                    EvaluationTarget {
                        sheet: usize::MAX,
                        ..at("A1")
                    },
                    3.,
                ),
                1 => edits.set_formula(at("A2"), ""),
                _ => edits.set_cached_value(at("Z99"), 3.),
            }
        });
        assert!(result.is_err());
        assert!(overlay.cached_value(at("C1")).is_none());
        let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
        let (baseline, state) =
            Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
        let restored = state.attach(baseline).unwrap();
        assert!(restored.cached_value(at("C1")).is_none());
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("C1"))
                .unwrap(),
            CellValue::Number(f64::from(failure) + 15.)
        );
        assert_eq!(
            overlay.source_cell(at("A2")).unwrap().value,
            CellValue::Number(2.)
        );
    }
}

#[test]
fn unwinding_also_flushes_successful_edits() {
    let book = workbook();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("C1"))
        .unwrap();
    let failure = std::panic::catch_unwind(AssertUnwindSafe(|| {
        overlay.edit(|edits| {
            edits.set_value(at("A1"), 10.).unwrap();
            panic!("interrupt edit closure");
        });
    }));
    assert!(failure.is_err());
    assert!(overlay.cached_value(at("C1")).is_none());
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(15.)
    );
}

#[test]
fn batch_handles_range_insertions_lazy_branches_and_spill_resizing() {
    let mut book = workbook();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_formula(at("G1").address, "SEQUENCE(A1)").unwrap();
    sheet.set_formula(at("J1").address, "SUM(G1#)").unwrap();
    sheet.set_formula(at("K1").address, "G3").unwrap();
    let context = EvaluationContext::default();
    let policy = OverlayPolicy {
        arrays: ArrayResultPolicy::DynamicSpill,
        ..Default::default()
    };
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy)
        .build()
        .unwrap();
    let outputs = [at("C1"), at("J1"), at("K1")];
    overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&outputs)
        .unwrap();
    overlay.edit(|edits| {
        edits.set_value(at("A1"), 3.).unwrap();
        edits.remove_cell(at("A2")).unwrap();
        edits.set_value(at("A3"), 4.).unwrap();
    });
    let values = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&outputs)
        .unwrap();
    assert_eq!(
        values.iter().map(|o| o.value.clone()).collect::<Vec<_>>(),
        [7., 6., 3.].map(CellValue::Number)
    );
    overlay.edit(|edits| {
        edits.set_value(at("A1"), -1.).unwrap();
        edits.set_value(at("A2"), 5.).unwrap();
    });
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(4.)
    );
    overlay.edit(|edits| {
        edits.set_value(at("A1"), 2.).unwrap();
    });
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("K1"))
            .unwrap(),
        CellValue::Number(0.)
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("J1"))
            .unwrap(),
        CellValue::Number(3.)
    );
}
