use poirot::calculation;
use poirot::eval::{EvaluationContext, EvaluationLimits, EvaluationTarget, overlay};
use poirot::model::{CellAddress, CellStore, CellValue, Workbook};
use std::sync::Arc;

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

fn workbook() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1", 2.).unwrap();
    sheet.set_formula("B1", "A1*2").unwrap();
    sheet.set_formula("C1", "SUM(A1:A100)").unwrap();
    sheet.set_formula("D1", "7*3").unwrap();
    sheet.set_formula("E1", "B1+C1").unwrap();
    book
}

#[test]
fn clone_preserves_warm_cache_and_independent_dirty_range_dependencies() {
    let book = workbook();
    let context = EvaluationContext::default();
    let mut original = overlay::Overlay::with_workbook(&book).build().unwrap();
    original.evaluation(&context).unwrap().all().unwrap();
    let mut sibling = original.clone();
    assert_eq!(sibling.retained_usage(), original.retained_usage());
    assert_eq!(sibling.compiled_usage(), original.compiled_usage());
    assert_eq!(sibling.changes(), original.changes());
    assert!(std::ptr::eq(sibling.workbook(), &book));
    let mut run = sibling.evaluation(&context).unwrap();
    assert_eq!(run.cell(at("E1")).unwrap(), CellValue::Number(6.));
    assert_eq!(run.usage().formula_executions, 0);
    assert!(run.usage().external_cache_hits > 0);
    drop(run);

    original.set_value(at("A50"), 10.).unwrap();
    assert!(original.cached_value(at("C1")).is_none());
    assert!(sibling.cached_value(at("C1")).is_some());
    let mut dirty = original.clone();
    assert_eq!(dirty.retained_usage(), original.retained_usage());
    assert!(dirty.cached_value(at("C1")).is_none());
    sibling.set_value(at("A1"), 5.).unwrap();
    dirty.set_value(at("A50"), 20.).unwrap();
    for (branch, expected) in [(&mut original, 16.), (&mut sibling, 15.), (&mut dirty, 26.)] {
        let mut run = branch.evaluation(&context).unwrap();
        assert_eq!(run.cell(at("D1")).unwrap(), CellValue::Number(21.));
        assert_eq!(run.usage().formula_executions, 0);
        assert_eq!(run.cell(at("E1")).unwrap(), CellValue::Number(expected));
    }
    original.remove_cell(at("A50")).unwrap();
    assert_eq!(
        original
            .evaluation(&context)
            .unwrap()
            .cell(at("E1"))
            .unwrap(),
        CellValue::Number(6.)
    );
    assert_eq!(
        *dirty.cached_value(at("E1")).unwrap(),
        CellValue::Number(26.)
    );
}

#[test]
fn clone_preserves_dynamic_spills_and_isolates_resize_and_blockers() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1", 3.).unwrap();
    sheet.set_formula("C1", "SEQUENCE(A1)").unwrap();
    let context = EvaluationContext::default();
    let mut original = overlay::Overlay::with_workbook(book).build().unwrap();
    assert_eq!(
        original
            .evaluation(&context)
            .unwrap()
            .cell(at("C3"))
            .unwrap(),
        CellValue::Number(3.)
    );
    let mut resized = original.clone();
    resized.set_value(at("A1"), 1.).unwrap();
    let mut dirty = resized.clone();
    assert_eq!(
        resized
            .evaluation(&context)
            .unwrap()
            .cell(at("C3"))
            .unwrap(),
        CellValue::Blank
    );
    assert_eq!(
        dirty.evaluation(&context).unwrap().cell(at("C3")).unwrap(),
        CellValue::Blank
    );
    assert_eq!(resized.changes(), dirty.changes());
    original.set_value(at("C2"), 99.).unwrap();
    assert_eq!(
        original
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Error(poirot::model::CellError::Spill)
    );
    assert_eq!(
        resized
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(1.)
    );
    dirty.set_value(at("A1"), 4.).unwrap();
    assert_eq!(
        dirty.evaluation(&context).unwrap().cell(at("C4")).unwrap(),
        CellValue::Number(4.)
    );
    assert_eq!(
        resized
            .evaluation(&context)
            .unwrap()
            .cell(at("C4"))
            .unwrap(),
        CellValue::Blank
    );
}

#[test]
fn owned_baselines_copy_and_arc_baselines_remain_shared() {
    let mut original = overlay::Overlay::with_workbook(workbook()).build().unwrap();
    original.set_value(at("A1"), 8.).unwrap();
    let mut sibling = original.clone();
    sibling.flush().unwrap();
    assert_eq!(
        sibling
            .workbook()
            .stored_cell("S", "A1")
            .unwrap()
            .unwrap()
            .value,
        CellValue::Number(8.)
    );
    assert_eq!(
        original
            .workbook()
            .stored_cell("S", "A1")
            .unwrap()
            .unwrap()
            .value,
        CellValue::Number(2.)
    );
    assert_eq!(original.edited_cells(), 1);
    assert_eq!(sibling.edited_cells(), 0);
    let shared = Arc::new(workbook());
    let first = overlay::Overlay::with_workbook(Arc::clone(&shared))
        .build()
        .unwrap();
    let second = first.clone();
    assert!(Arc::ptr_eq(&first.into_workbook(), &second.into_workbook()));
}

#[test]
fn public_wrapper_clones_preserve_cache_and_scenario_edits() {
    let context = EvaluationContext::default();
    let mut first = workbook().into_overlay().build().unwrap();
    assert_eq!(
        first.evaluation(&context).unwrap().cell(at("E1")).unwrap(),
        CellValue::Number(6.)
    );
    let mut second = first.clone();
    second.set_value(at("A1"), 9.).unwrap();
    assert_eq!(
        first.evaluation(&context).unwrap().cell(at("E1")).unwrap(),
        CellValue::Number(6.)
    );
    assert_eq!(
        second.evaluation(&context).unwrap().cell(at("E1")).unwrap(),
        CellValue::Number(27.)
    );
    let mut direct = workbook().into_evaluation().build().unwrap();
    direct.cell(at("E1")).unwrap();
    let mut copied = direct.clone();
    assert_eq!(copied.cached_cells(), direct.cached_cells());
    copied.set_value(at("A1"), 4.).unwrap();
    assert_eq!(copied.cell(at("E1")).unwrap(), CellValue::Number(12.));
    assert_eq!(direct.cell(at("E1")).unwrap(), CellValue::Number(6.));
}

#[test]
fn evaluation_clone_keeps_consumed_budget_and_failure_until_reset() {
    let book = workbook();
    let mut control = overlay::Evaluation::with_workbook(&book)
        .build(EvaluationContext::default())
        .unwrap();
    control.cell(at("B1")).unwrap();
    let consumed = control.finish().outcome.usage().work_units;
    assert!(consumed > 0);
    let context = EvaluationContext {
        limits: EvaluationLimits::default().max_work(consumed),
        ..Default::default()
    };
    let mut original = overlay::Evaluation::with_workbook(&book)
        .build(context)
        .unwrap();
    original.cell(at("B1")).unwrap();
    let mut sibling = original.clone();
    assert_eq!(original.usage(), sibling.usage());
    assert_eq!(
        original.clone().finish().outcome.usage(),
        sibling.clone().finish().outcome.usage()
    );
    let failure = sibling.cell(at("D1")).unwrap_err();
    assert!(matches!(
        original.finish().outcome,
        poirot::eval::EvaluationOutcome::Complete { .. }
    ));
    let mut failed = sibling.clone();
    assert_eq!(
        failed.cell(at("B1")).unwrap_err().budget().unwrap(),
        failure.budget().unwrap()
    );
    sibling.begin(EvaluationContext::default()).unwrap();
    assert_eq!(sibling.cell(at("D1")).unwrap(), CellValue::Number(21.));
    assert_eq!(
        failed.cell(at("D1")).unwrap_err().budget().unwrap(),
        failure.budget().unwrap()
    );
}

#[test]
fn borrowed_cursors_clone_without_clone_payloads_at_the_current_position() {
    #[derive(Debug, PartialEq)]
    struct Payload(u32);
    let mut store: CellStore<Payload> = CellStore::default();
    for row in [1, 2, 33, 99] {
        store
            .set(CellAddress::new(row, 1).unwrap(), Payload(row))
            .unwrap();
    }
    let mut iter = store.cells().iter();
    iter.next();
    iter.next_back();
    assert_eq!(iter.clone().collect::<Vec<_>>(), iter.collect::<Vec<_>>());
    let mut range = store.cells().range(..);
    range.next_back();
    range.next();
    assert_eq!(
        range.clone().rev().collect::<Vec<_>>(),
        range.rev().collect::<Vec<_>>()
    );
    let mut column = store.cells().column(1, 1, 99);
    column.next();
    assert_eq!(
        column.clone().collect::<Vec<_>>(),
        column.collect::<Vec<_>>()
    );
    let mut rows = store.cells().occupied_rows(1, 99);
    rows.next();
    rows.next_back();
    assert_eq!(rows.clone().collect::<Vec<_>>(), rows.collect::<Vec<_>>());
}

#[test]
fn public_clone_bounds_cover_baseline_ownership_and_retained_state() {
    fn cloneable<T: Clone>() {}
    cloneable::<overlay::Overlay<'static, Workbook>>();
    cloneable::<overlay::Overlay<'static, &'static Workbook>>();
    cloneable::<overlay::Overlay<'static, Arc<Workbook>>>();
    cloneable::<overlay::Evaluation<'static, Workbook>>();
    cloneable::<overlay::Calculation<'static, Workbook>>();
    cloneable::<overlay::OverlayBuilder<'static, Workbook>>();
    cloneable::<overlay::EvaluationBuilder<'static, Workbook>>();
    cloneable::<calculation::Overlay<'static, Workbook>>();
    cloneable::<calculation::Evaluation<'static, Workbook>>();
    cloneable::<calculation::OverlayBuilder<'static, Workbook>>();
    cloneable::<calculation::EvaluationBuilder<'static, Workbook>>();
    cloneable::<overlay::WorkspaceState>();
    cloneable::<poirot::eval::FormulaCompiler>();
    cloneable::<poirot::eval::RunInputs>();
    cloneable::<poirot::eval::run::RetainedRun<Vec<CellValue>>>();
    cloneable::<poirot::eval::observe::BoundedExecutionTrace>();
    cloneable::<poirot::snapshots::RestoredWorkbook>();
    cloneable::<poirot::snapshots::RestoredEvaluation>();
    cloneable::<poirot::model::query::WorkbookView<'static, dyn poirot::model::query::WorkbookRead>>(
    );
}

#[test]
fn compiler_clone_shares_programs_but_keeps_independent_membership_and_usage() {
    let mut original = poirot::eval::FormulaCompiler::new();
    let policy = Default::default();
    let prepared = original.compile_formula("1+2", &policy);
    let usage = original.usage().clone();
    let mut sibling = original.clone();
    assert_eq!(sibling.usage(), &usage);
    assert!(Arc::ptr_eq(
        &prepared,
        &sibling.compile_formula("1+2", &policy)
    ));
    sibling.compile_formula("SUM(A1:A100)", &policy);
    assert_eq!(original.usage(), &usage);
    assert!(sibling.usage().parse_attempts > original.usage().parse_attempts);
}

#[test]
fn restored_clone_keeps_warm_results_and_isolates_new_edits() {
    let context = EvaluationContext::default();
    let mut original = workbook().into_overlay().build().unwrap();
    original.set_value(at("A1"), 4.).unwrap();
    original
        .evaluation(&context)
        .unwrap()
        .cell(at("E1"))
        .unwrap();
    let bytes = original.save_snapshot().to_buffer().unwrap();
    let restored = Workbook::restore_snapshot().from_bytes(&bytes).unwrap();
    let mut left = restored.clone().into_overlay().unwrap();
    let mut right = restored.into_overlay().unwrap();
    assert_eq!(
        left.cached_value(at("E1")).as_deref(),
        Some(&CellValue::Number(12.))
    );
    left.set_value(at("A1"), 9.).unwrap();
    assert_eq!(
        left.evaluation(&context).unwrap().cell(at("E1")).unwrap(),
        CellValue::Number(27.)
    );
    assert_eq!(
        right.evaluation(&context).unwrap().cell(at("E1")).unwrap(),
        CellValue::Number(12.)
    );
}

#[test]
fn trace_clone_preserves_retention_limits_and_failure_independently() {
    use poirot::eval::observe::{
        BoundedExecutionTrace, ExecutionEvent, ExecutionObserver, Operation, OperationId,
        OperationKind, RetentionLimits,
    };
    let event = ExecutionEvent::Begin(Operation {
        id: OperationId(1),
        parent: None,
        child_sequence: 0,
        target: at("A1"),
        kind: OperationKind::Expression { handler: None },
    });
    let mut original = BoundedExecutionTrace::new(RetentionLimits {
        events: 2,
        scalar_values: 0,
        text_bytes: 0,
    });
    original.observe(event).unwrap();
    let mut sibling = original.clone();
    sibling.observe(event).unwrap();
    assert_eq!(original.events().len(), 1);
    assert_eq!(sibling.events().len(), 2);
    assert!(sibling.observe(event).is_err());
    assert!(sibling.is_partial());
    assert!(!original.is_partial());
    let mut rejected = sibling.clone();
    assert!(rejected.is_partial());
    assert!(rejected.observe(event).is_err());
    original.observe(event).unwrap();
}
