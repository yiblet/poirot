use super::*;
use std::time::Duration;
pub(super) fn at(reference: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: reference.parse().unwrap(),
    }
}
pub(super) fn book() -> Workbook {
    let mut book = Workbook::new();
    let s = book.add_sheet("S").unwrap();
    s.set_value(at("A1").address, 2.).unwrap();
    s.set_formula(at("B1").address, "A1*2").unwrap();
    s.set_formula(at("C1").address, "B1+1").unwrap();
    s.set_formula(at("D1").address, "7*3").unwrap();
    book
}
pub(super) fn value<W: Borrow<Workbook>>(
    overlay: &mut Overlay<'_, W>,
    cell: &str,
    context: &EvaluationContext,
) -> CellValue {
    overlay.evaluation(context).unwrap().cell(at(cell)).unwrap()
}
#[test]
fn lazy_compilation_and_shared_session_outputs() {
    let book = book();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    assert!(std::ptr::eq(overlay.workbook(), &book));
    assert_eq!(overlay.compiled_usage().parse_attempts, 0);
    let context = EvaluationContext::default();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    let result = evaluation.outputs(&[at("C1"), at("B1"), at("C1")]).unwrap();
    assert_eq!(
        result.iter().map(|r| r.value.clone()).collect::<Vec<_>>(),
        vec![
            CellValue::Number(5.),
            CellValue::Number(4.),
            CellValue::Number(5.)
        ]
    );
    assert_eq!(evaluation.usage().formula_executions, 2);
    drop(evaluation);
    assert_eq!(overlay.edited_cells(), 0);
    assert_eq!(overlay.cached_cells(), 2);
    // Only B1 and C1 lie in the demanded source prefix; D1 remains untouched.
    assert_eq!(overlay.compiled_usage().parse_attempts, 2);
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("C1")).unwrap(), CellValue::Number(5.));
    assert_eq!(next.usage().formula_executions, 0);
    assert_eq!(next.usage().external_cache_hits, 1);
    assert_eq!(next.cell(at("D1")).unwrap(), CellValue::Number(21.));
    drop(next);
    assert_eq!(overlay.compiled_usage().parse_attempts, 3);
}
#[test]
fn writes_invalidate_recursive_dependents_and_preserve_unrelated_results() {
    let book = book();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    overlay.evaluation(&context).unwrap().all().unwrap();
    assert_eq!(overlay.cached_cells(), 3);
    overlay.set_value(at("A1"), 8.).unwrap();
    assert!(overlay.cached_value(at("B1")).is_none());
    assert!(overlay.cached_value(at("C1")).is_none());
    assert_eq!(
        *overlay.cached_value(at("D1")).unwrap(),
        CellValue::Number(21.)
    );
    assert_eq!(value(&mut overlay, "C1", &context), CellValue::Number(17.));
    overlay.remove_cell(at("A1")).unwrap();
    assert!(overlay.cached_value(at("C1")).is_none());
    assert_eq!(value(&mut overlay, "C1", &context), CellValue::Number(1.));
}
#[test]
fn range_membership_tracks_new_cells_and_removals() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("E1").address, "SUM(A2:A10)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(value(&mut overlay, "E1", &context), CellValue::Number(0.));
    overlay.set_value(at("A5"), 9.).unwrap();
    assert!(overlay.cached_value(at("E1")).is_none());
    assert_eq!(value(&mut overlay, "E1", &context), CellValue::Number(9.));
    overlay.remove_cell(at("A5")).unwrap();
    assert_eq!(value(&mut overlay, "E1", &context), CellValue::Number(0.));
    overlay.set_value(at("B5"), 88.).unwrap();
    assert!(overlay.cached_value(at("E1")).is_some());
}
#[test]
fn branch_reexecution_replaces_reverse_dependencies() {
    let mut book = book();
    let s = book.sheet_mut(0).unwrap();
    s.set_value(at("A2").address, true).unwrap();
    s.set_value(at("B2").address, 4.).unwrap();
    s.set_value(at("C2").address, 9.).unwrap();
    s.set_formula(at("D2").address, "IF(A2,B2,C2)").unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(value(&mut overlay, "D2", &context), CellValue::Number(4.));
    overlay.set_value(at("C2"), 10.).unwrap();
    assert!(overlay.cached_value(at("D2")).is_some());
    overlay.set_value(at("A2"), false).unwrap();
    assert_eq!(value(&mut overlay, "D2", &context), CellValue::Number(10.));
    overlay.set_value(at("B2"), 20.).unwrap();
    assert!(overlay.cached_value(at("D2")).is_some());
    overlay.set_value(at("C2"), 30.).unwrap();
    assert!(overlay.cached_value(at("D2")).is_none());
}
#[test]
fn formula_replacement_and_return_to_base_drop_redundant_writes() {
    let book = book();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    value(&mut overlay, "C1", &context);
    overlay.set_formula(at("B1"), "=A1*3").unwrap();
    assert_eq!(value(&mut overlay, "C1", &context), CellValue::Number(7.));
    overlay.set_value(at("A1"), 9.).unwrap();
    overlay.set_value(at("A1"), 2.).unwrap();
    assert_eq!(overlay.edited_cells(), 1);
    overlay.set_formula(at("B1"), "A1*2").unwrap();
    assert_eq!(overlay.edited_cells(), 0);
}
#[test]
fn unchanged_captured_randomness_and_dependents_reuse() {
    let mut book = book();
    let s = book.sheet_mut(0).unwrap();
    s.set_formula(at("E1").address, "RAND()").unwrap();
    s.set_formula(at("F1").address, "E1+1").unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&[at("F1"), at("D1")])
        .unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    evaluation.outputs(&[at("F1"), at("D1")]).unwrap();
    assert_eq!(evaluation.usage().formula_executions, 0);
    assert_eq!(evaluation.usage().external_cache_hits, 2);
}
#[test]
fn snapshots_preserve_sources_programs_results_and_dependency_invalidation() {
    let book = book();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    overlay.set_value(at("A1"), 5.).unwrap();
    assert_eq!(value(&mut overlay, "C1", &context), CellValue::Number(11.));
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let snapshot: snapshot::Snapshot<'static> = postcard::from_bytes(&bytes).unwrap();
    drop(overlay);
    drop(book);
    let (restored_workbook, restored_state) = Overlay::from_snapshot(snapshot).unwrap();
    let mut restored = restored_state.attach(&restored_workbook).unwrap();
    assert_eq!(restored.compiled_usage().parse_attempts, 0);
    assert_eq!(value(&mut restored, "C1", &context), CellValue::Number(11.));
    assert_eq!(restored.compiled_usage().parse_attempts, 0);
    restored.set_value(at("A1"), 7.).unwrap();
    assert!(restored.cached_value(at("C1")).is_none());
    assert_eq!(value(&mut restored, "C1", &context), CellValue::Number(15.));
    assert_eq!(restored.compiled_usage().parse_attempts, 0);
}
#[test]
fn owned_changes_outlive_overlay_and_preserve_formula_source() {
    let mut book = book();
    let context = EvaluationContext::default();
    let changes = {
        let mut overlay = Overlay::with_workbook(&book).build().unwrap();
        overlay.set_value(at("A1"), 9.).unwrap();
        value(&mut overlay, "C1", &context);
        overlay.changes()
    };
    fn is_static<T: 'static>(_: &T) {}
    is_static(&changes);
    book.apply(changes).unwrap();
    let s = book.sheet(0).unwrap();
    assert_eq!(
        s.get(at("A1").address).unwrap().value,
        CellValue::Number(9.)
    );
    assert_eq!(
        s.get(at("C1").address).unwrap().value,
        CellValue::Number(19.)
    );
    assert_eq!(
        s.get(at("C1").address).unwrap().formula.as_deref(),
        Some("B1+1")
    );
}
#[test]
fn evaluation_changes_are_separate_from_overlay_source_edits() {
    let mut book = book();
    let context = EvaluationContext::default();
    let changes = {
        let mut overlay = Overlay::with_workbook(&book).build().unwrap();
        overlay.set_value(at("A1"), 9.).unwrap();
        let mut evaluation = overlay.evaluation(&context).unwrap();
        assert_eq!(evaluation.cell(at("C1")).unwrap(), CellValue::Number(19.));
        let results = evaluation.result_changes().unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().all(|c| matches!(
            c,
            WorkbookChange::SetCachedValue { .. } | WorkbookChange::ClearSpillCache { .. }
        )));
        assert_eq!(evaluation.into_result_changes().unwrap(), results);
        let changes = overlay.changes();
        assert!(changes.iter().any(|c| matches!(
            c,
            WorkbookChange::SetValue { address, value: CellValue::Number(9.), .. }
                if *address == at("A1").address
        )));
        assert_eq!(overlay.into_changes(), changes);
        changes
    };
    book.apply(changes).unwrap();
    let sheet = book.sheet(0).unwrap();
    assert_eq!(
        sheet.get(at("A1").address).unwrap().value,
        CellValue::Number(9.)
    );
    assert_eq!(
        sheet.get(at("C1").address).unwrap().value,
        CellValue::Number(19.)
    );
}

#[test]
fn failed_output_batch_publishes_no_partial_result_cache() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("E1").address, "REPT(\"x\",100)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext {
        limits: crate::eval::EvaluationLimits::default().max_text_bytes(30),
        ..Default::default()
    };
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert!(evaluation.outputs(&[at("C1"), at("E1")]).is_err());
    assert!(evaluation.result_changes().unwrap().is_empty());
    assert!(evaluation.cell(at("D1")).is_err());
    drop(evaluation);
    assert_eq!(overlay.cached_cells(), 0);
}
#[test]
fn malformed_formulas_can_be_snapshotted_without_eager_failure() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("E1").address, "1+")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    overlay.compile_all();
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let snapshot = postcard::from_bytes(&bytes).unwrap();
    let (overlay_workbook, overlay_state) = Overlay::from_snapshot(snapshot).unwrap();
    let mut overlay = overlay_state.attach(&overlay_workbook).unwrap();
    assert_eq!(
        value(&mut overlay, "E1", &EvaluationContext::default()),
        CellValue::Error(crate::model::CellError::Value)
    );
    assert_eq!(overlay.compiled_usage().parse_attempts, 0);
}
#[test]
fn compiler_and_cache_stay_empty_before_use() {
    let book = book();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    assert_eq!(overlay.edited_cells(), 0);
    assert_eq!(overlay.cached_cells(), 0);
    assert_eq!(overlay.compiled_usage().parse_attempts, 0);
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    assert!(!bytes.is_empty());
}

pub(super) fn dynamic() -> EvaluationPolicy {
    EvaluationPolicy {
        builtin: crate::eval::BuiltinSemanticsPolicy::ExcelDocumentation,
        arrays: crate::eval::ArrayResultPolicy::DynamicSpill,
        ..Default::default()
    }
}
#[test]
fn new_overlay_spill_formula_is_discovered_child_first_and_blockers_invalidate_footprints() {
    let book = book();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(dynamic())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    overlay.set_formula(at("G1"), "{1,2;3,4}").unwrap();
    assert_eq!(value(&mut overlay, "H2", &context), CellValue::Number(4.));
    assert!(overlay.cached_value(at("G1")).is_some());
    overlay.set_value(at("H2"), 99.).unwrap();
    assert!(overlay.cached_value(at("G1")).is_none());
    assert!(overlay.cached_value(at("G2")).is_none());
    assert_eq!(
        value(&mut overlay, "G1", &context),
        CellValue::Error(crate::model::CellError::Spill)
    );
    overlay.remove_cell(at("H2")).unwrap();
    assert!(overlay.cached_value(at("G1")).is_none());
    assert_eq!(value(&mut overlay, "H2", &context), CellValue::Number(4.));
}
#[test]
fn spill_shrink_removes_old_children_and_invalidates_consumers() {
    let book = book();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(dynamic())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    overlay
        .set_formula(at("G1"), "IF(A1,{1,2;3,4},{9})")
        .unwrap();
    overlay.set_formula(at("J1"), "SUM(H1:H2)").unwrap();
    assert_eq!(value(&mut overlay, "J1", &context), CellValue::Number(6.));
    overlay.set_value(at("A1"), false).unwrap();
    assert!(overlay.cached_value(at("J1")).is_none());
    assert_eq!(value(&mut overlay, "H2", &context), CellValue::Blank);
    assert_eq!(value(&mut overlay, "J1", &context), CellValue::Number(0.));
}
#[test]
fn intersection_reads_only_the_selected_overlap() {
    let mut book = book();
    let s = book.sheet_mut(0).unwrap();
    s.set_value(at("A2").address, 2.).unwrap();
    s.set_value(at("A3").address, 3.).unwrap();
    s.set_formula(at("E1").address, "SUM(A1:A3 A2:B2)").unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(value(&mut overlay, "E1", &context), CellValue::Number(2.));
    overlay.set_value(at("A3"), 99.).unwrap();
    assert!(overlay.cached_value(at("E1")).is_some());
    overlay.set_value(at("A2"), 8.).unwrap();
    assert!(overlay.cached_value(at("E1")).is_none());
    assert_eq!(value(&mut overlay, "E1", &context), CellValue::Number(8.));
}
#[test]
fn implicit_intersection_tracks_the_selected_cell_and_context_fields_are_selective() {
    let mut book = book();
    let s = book.sheet_mut(0).unwrap();
    s.set_value(at("A2").address, 4.).unwrap();
    s.set_formula(at("E2").address, "@A1:A3").unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let mut context = EvaluationContext::default();
    assert_eq!(value(&mut overlay, "E2", &context), CellValue::Number(4.));
    overlay.set_value(at("A1"), 66.).unwrap();
    assert!(overlay.cached_value(at("E2")).is_some());
    context.as_of_time += Duration::from_secs(86400);
    let mut evaluation = overlay.evaluation(&context).unwrap();
    evaluation.cell(at("E2")).unwrap();
    assert_eq!(evaluation.usage().formula_executions, 0);
    drop(evaluation);
    overlay.set_value(at("A2"), 7.).unwrap();
    assert!(overlay.cached_value(at("E2")).is_none());
}
#[test]
fn cached_spill_ownership_survives_evaluations_snapshots_and_workbook_writeback() {
    let mut book = book();
    let context = EvaluationContext::default();
    let changes = {
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(dynamic())
            .build()
            .unwrap();
        overlay.set_formula(at("G1"), "{1,2;3,4}").unwrap();
        assert_eq!(value(&mut overlay, "H2", &context), CellValue::Number(4.));
        let mut evaluation = overlay.evaluation(&context).unwrap();
        assert_eq!(evaluation.cell(at("H2")).unwrap(), CellValue::Number(4.));
        assert_eq!(evaluation.usage().formula_executions, 0);
        drop(evaluation);
        let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
        let snapshot = postcard::from_bytes(&bytes).unwrap();
        let (restored_workbook, restored_state) = Overlay::from_snapshot(snapshot).unwrap();
        let mut restored = restored_state.attach(&restored_workbook).unwrap();
        let mut evaluation = restored.evaluation(&context).unwrap();
        assert_eq!(evaluation.cell(at("H2")).unwrap(), CellValue::Number(4.));
        assert_eq!(evaluation.usage().formula_executions, 0);
        drop(evaluation);
        overlay.changes()
    };
    book.apply(changes).unwrap();
    assert_eq!(
        book.sheet(0).unwrap().spill_owner(at("H2").address),
        Some(at("G1").address)
    );
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(dynamic())
        .build()
        .unwrap();
    assert_eq!(value(&mut overlay, "H2", &context), CellValue::Number(4.));
    overlay.set_formula(at("G1"), "{9}").unwrap();
    assert_eq!(value(&mut overlay, "H2", &context), CellValue::Blank);
    assert_eq!(value(&mut overlay, "G1", &context), CellValue::Number(9.));
    let changes = overlay.changes();
    drop(overlay);
    book.apply(changes).unwrap();
    assert!(book.sheet(0).unwrap().get(at("H2").address).is_none());
    let bytes = postcard::to_allocvec(&book.snapshot()).unwrap();
    let snapshot = postcard::from_bytes(&bytes).unwrap();
    let book = Workbook::from_snapshot(snapshot).unwrap();
    assert_eq!(
        book.sheet(0).unwrap().get(at("G1").address).unwrap().value,
        CellValue::Number(9.)
    );
}
#[test]
fn inserting_a_producer_outside_a_consumed_range_invalidates_the_range_reader() {
    let book = book();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(dynamic())
        .build()
        .unwrap();
    overlay.set_formula(at("J1"), "SUM(H1:H2)").unwrap();
    assert_eq!(value(&mut overlay, "J1", &context), CellValue::Number(0.));
    overlay.set_formula(at("G1"), "{1,2;3,4}").unwrap();
    assert!(overlay.cached_value(at("J1")).is_none());
    assert_eq!(value(&mut overlay, "J1", &context), CellValue::Number(6.));
}
#[test]
fn reused_spill_footprints_keep_input_dependencies_after_warm_evaluation() {
    let book = book();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(dynamic())
        .build()
        .unwrap();
    overlay
        .set_formula(at("G1"), "IF(A1,{1,2;3,4},{9})")
        .unwrap();
    assert_eq!(value(&mut overlay, "H2", &context), CellValue::Number(4.));
    assert_eq!(value(&mut overlay, "H2", &context), CellValue::Number(4.));
    overlay.set_value(at("A1"), false).unwrap();
    assert!(overlay.cached_value(at("G1")).is_none());
    assert!(overlay.cached_value(at("H2")).is_none());
    assert_eq!(value(&mut overlay, "H2", &context), CellValue::Blank);
}
#[test]
fn publication_policy_reuses_finite_results_and_keeps_cached_identity() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("E1").address, "NORM.S.INV(0.5)")
        .unwrap();
    let policy = EvaluationPolicy {
        ..Default::default()
    };
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy)
        .build()
        .unwrap();
    assert_eq!(value(&mut overlay, "E1", &context), CellValue::Number(0.));
    let original = overlay.cached_value(at("E1")).unwrap();
    let mut policy = overlay.policy().clone();
    policy.materialization = crate::eval::FormulaCacheMaterializationPolicy::PinnedPoiCellSetter;
    overlay.set_policy(policy).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("E1")).unwrap(), CellValue::Number(0.));
    assert_eq!(evaluation.usage().formula_executions, 0);
    assert!(
        evaluation
            .result_changes()
            .unwrap()
            .iter()
            .any(|c| matches!(
                c,
                WorkbookChange::SetCachedValue {
                    value: CellValue::Number(0.),
                    ..
                }
            ))
    );
    drop(evaluation);
    assert!(Arc::ptr_eq(
        &original,
        &overlay.cached_value(at("E1")).unwrap()
    ));
}
#[test]
fn explicit_input_storage_representation_is_preserved_even_when_units_match() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_value(
            at("A2").address,
            CellValue::TextUtf16(crate::model::ExcelText::from("A")),
        )
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    overlay.set_value(at("A2"), "A").unwrap();
    assert!(matches!(
        value(&mut overlay, "A2", &EvaluationContext::default()),
        CellValue::Text(_)
    ));
    let changes = overlay.changes();
    drop(overlay);
    book.apply(changes).unwrap();
    assert!(matches!(
        book.sheet(0).unwrap().get(at("A2").address).unwrap().value,
        CellValue::Text(_)
    ));
}

#[test]
fn reused_lookup_prefixes_record_dependencies_for_every_consumer() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for row in 0..200 {
        sheet
            .set_value(CellAddress { row, col: 0 }, f64::from(row + 1))
            .unwrap();
    }
    sheet
        .set_formula(at("C1").address, "MATCH(100,A1:A200,0)")
        .unwrap();
    sheet
        .set_formula(at("C2").address, "MATCH(50,A1:A200,0)")
        .unwrap();
    sheet.set_formula(at("D2").address, "C2*2").unwrap();
    sheet
        .set_formula(at("C3").address, "MATCH(300,A1:A200,0)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
            arrays: crate::eval::ArrayResultPolicy::ScalarIntersection,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    evaluation.outputs(&[at("C1"), at("D2"), at("C3")]).unwrap();
    assert!(
        evaluation.usage().session_memo_hits < 5,
        "prefix reuse should avoid rescanning candidates"
    );
    drop(evaluation);
    overlay.set_value(at("A150"), 300.).unwrap();
    assert!(overlay.cached_value(at("C1")).is_some());
    assert!(overlay.cached_value(at("C2")).is_some());
    assert!(overlay.cached_value(at("C3")).is_none());
    assert_eq!(value(&mut overlay, "C3", &context), CellValue::Number(150.));
    overlay.set_value(at("A25"), 50.).unwrap();
    assert!(overlay.cached_value(at("C2")).is_none());
    assert!(overlay.cached_value(at("D2")).is_none());
    assert_eq!(value(&mut overlay, "D2", &context), CellValue::Number(50.));
    let snapshot = overlay.snapshot().into_owned().unwrap();
    let (restored_workbook, restored_state) = Overlay::from_snapshot(snapshot).unwrap();
    let mut restored = restored_state.attach(&restored_workbook).unwrap();
    restored.set_value(at("A24"), 50.).unwrap();
    assert_eq!(value(&mut restored, "D2", &context), CellValue::Number(48.));
}

#[test]
fn successful_writeback_survives_a_later_failed_call() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("E1").address, "REPT(\"x\",100)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext {
        limits: crate::eval::EvaluationLimits::default().max_text_bytes(50),
        ..Default::default()
    };
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("C1")).unwrap(), CellValue::Number(5.));
    let before = evaluation.result_changes().unwrap();
    assert!(!before.is_empty());
    assert!(evaluation.cell(at("E1")).is_err());
    assert_eq!(evaluation.result_changes().unwrap(), before);
}

#[test]
fn snapshots_exclude_runtime_providers_and_require_matching_registry() {
    use crate::eval::extensions::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = Arc::clone(&calls);
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "USER_VALUE",
            "value/v1",
            ExtensionContract {
                arity: crate::eval::registry::Arity {
                    minimum: 0,
                    maximum: 0,
                },
                evaluation: ExtensionEvaluationStrategy::Eager,
                capabilities: ExtensionCapabilities {
                    references: false,
                    arrays: false,
                    context: false,
                },
                cacheability: ExtensionCacheability::Deterministic,
            },
            move |_host: &mut dyn ExtensionHost| {
                captured.fetch_add(1, Ordering::Relaxed);
                Ok(ExtensionOutput::Scalar(CellValue::Number(7.)))
            },
        )
        .unwrap();
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("E1").address, "USER_VALUE()")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_extensions(&registry)
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(value(&mut overlay, "E1", &context), CellValue::Number(7.));
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let snapshot = postcard::from_bytes(&bytes).unwrap();
    assert!(matches!(
        Overlay::from_snapshot(snapshot).and_then(|(workbook, state)| state.attach(workbook)),
        Err(OverlayError::RegistryMismatch)
    ));
    let snapshot = postcard::from_bytes(&bytes).unwrap();
    let (restored_workbook, restored_state) = Overlay::from_snapshot(snapshot).unwrap();
    let mut restored = restored_state
        .attach_with_extensions(&restored_workbook, &registry)
        .unwrap();
    assert_eq!(value(&mut restored, "E1", &context), CellValue::Number(7.));
    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "deterministic providers retain validated results after restoration"
    );
    assert_eq!(restored.compiled_usage().parse_attempts, 0);
}

#[test]
fn active_dependency_trace_preserves_reads_before_and_after_nested_formulas() {
    let mut book = book();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_value(at("A2").address, 3.).unwrap();
    sheet.set_value(at("A3").address, 5.).unwrap();
    sheet.set_formula(at("C1").address, "A2+B1+A3").unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    {
        let mut evaluation = overlay.evaluation(&context).unwrap();
        assert_eq!(evaluation.cell(at("C1")).unwrap(), CellValue::Number(12.));
        assert_eq!(evaluation.cell(at("D1")).unwrap(), CellValue::Number(21.));
    }
    overlay.set_value(at("A2"), 9.).unwrap();
    assert!(overlay.cached_value(at("B1")).is_some());
    assert!(overlay.cached_value(at("C1")).is_none());
    assert_eq!(value(&mut overlay, "C1", &context), CellValue::Number(18.));
    overlay.set_value(at("A3"), 7.).unwrap();
    assert!(overlay.cached_value(at("C1")).is_none());
    assert_eq!(value(&mut overlay, "C1", &context), CellValue::Number(20.));
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (restored_workbook, restored_state) =
        Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = restored_state.attach(&restored_workbook).unwrap();
    restored.set_value(at("A1"), 4.).unwrap();
    assert_eq!(value(&mut restored, "C1", &context), CellValue::Number(24.));
}

#[test]
fn captured_random_dependencies_track_branch_replacement_and_snapshot_restore() {
    let mut book = book();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_value(at("A2").address, false).unwrap();
    sheet
        .set_formula(at("E1").address, "IF(A2,RAND(),7)")
        .unwrap();
    sheet.set_formula(at("F1").address, "E1+1").unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(value(&mut overlay, "F1", &context), CellValue::Number(8.));
    {
        let mut evaluation = overlay.evaluation(&context).unwrap();
        assert_eq!(evaluation.cell(at("F1")).unwrap(), CellValue::Number(8.));
        assert_eq!(evaluation.usage().formula_executions, 0);
    }
    overlay.set_value(at("A2"), true).unwrap();
    value(&mut overlay, "F1", &context);
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (restored_workbook, restored_state) =
        Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = restored_state.attach(&restored_workbook).unwrap();
    {
        let mut evaluation = restored.evaluation(&context).unwrap();
        evaluation.cell(at("F1")).unwrap();
        assert_eq!(evaluation.usage().formula_executions, 0);
    }
    restored.set_value(at("A2"), false).unwrap();
    assert_eq!(value(&mut restored, "F1", &context), CellValue::Number(8.));
    let bytes = postcard::to_allocvec(&restored.snapshot()).unwrap();
    let (restored_workbook, restored_state) =
        Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = restored_state.attach(&restored_workbook).unwrap();
    let mut evaluation = restored.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("F1")).unwrap(), CellValue::Number(8.));
    assert_eq!(evaluation.usage().formula_executions, 0);
}

#[test]
fn conflicting_compiler_borrow_returns_snapshot_error() {
    let workbook = book();
    let overlay = Overlay::with_workbook(&workbook).build().unwrap();
    let _held = overlay.source.compiler.borrow_mut();
    assert!(postcard::to_allocvec(&overlay.snapshot()).is_err());
    assert!(overlay.snapshot().into_owned().is_err());
}

fn indexed_lookup_book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for row in 0..200 {
        sheet
            .set_value(CellAddress { row, col: 0 }, f64::from(row + 1))
            .unwrap();
        sheet
            .set_value(CellAddress { row, col: 1 }, f64::from((row + 1) * 10))
            .unwrap();
    }
    sheet.set_value(at("D1").address, 150.).unwrap();
    sheet
        .set_formula(at("E1").address, "VLOOKUP(D1,A1:B200,2,FALSE)")
        .unwrap();
    book
}

#[test]
fn lookup_indices_survive_new_evaluations_needle_and_return_value_edits() {
    let book = indexed_lookup_book();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
            arrays: crate::eval::ArrayResultPolicy::ScalarIntersection,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let mut first = overlay.evaluation(&context).unwrap();
    assert_eq!(first.cell(at("E1")).unwrap(), CellValue::Number(1500.));
    assert_eq!(first.usage().lookup_prefix_scans, 150);
    drop(first);
    overlay.set_value(at("D1"), 100.).unwrap();
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("E1")).unwrap(), CellValue::Number(1000.));
    assert_eq!(next.usage().formula_executions, 1);
    assert_eq!(next.usage().lookup_prefix_hits, 1);
    assert_eq!(next.usage().lookup_prefix_scans, 0);
    drop(next);
    overlay.set_value(at("B100"), 77.).unwrap();
    overlay.set_value(at("G1"), 55.).unwrap();
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("E1")).unwrap(), CellValue::Number(77.));
    assert_eq!(next.usage().lookup_prefix_hits, 1);
    assert_eq!(next.usage().lookup_prefix_scans, 0);
    drop(next);
    overlay.set_value(at("A50"), 100.).unwrap();
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("E1")).unwrap(), CellValue::Number(500.));
    assert_eq!(next.usage().lookup_prefix_scans, 50);
}

#[test]
fn lookup_indices_expire_for_transitively_changed_formula_candidates() {
    let mut book = indexed_lookup_book();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_value(at("G1").address, 50.).unwrap();
    sheet.set_formula(at("A50").address, "G1").unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
            arrays: crate::eval::ArrayResultPolicy::ScalarIntersection,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    value(&mut overlay, "E1", &context);
    overlay.set_value(at("G1"), 100.).unwrap();
    overlay.set_value(at("D1"), 100.).unwrap();
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("E1")).unwrap(), CellValue::Number(500.));
    assert_eq!(next.usage().lookup_prefix_scans, 50);
}

#[test]
fn lookup_indices_expire_for_sparse_gap_insertions_and_overlapping_ranges() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value(at("A100").address, 9.).unwrap();
    sheet.set_value(at("D1").address, 9.).unwrap();
    for (cell, formula) in [
        ("E1", "MATCH(D1,A1:A200,0)"),
        ("E2", "MATCH(D1,A20:A200,0)"),
    ] {
        sheet.set_formula(at(cell).address, formula).unwrap();
    }
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
            arrays: crate::eval::ArrayResultPolicy::ScalarIntersection,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&[at("E1"), at("E2")])
        .unwrap();
    // A previously absent coordinate belongs to both consumed search prefixes.
    overlay.set_value(at("A50"), 9.).unwrap();
    let mut next = overlay.evaluation(&context).unwrap();
    let results = next.outputs(&[at("E1"), at("E2")]).unwrap();
    assert_eq!(results[0].value, CellValue::Number(50.));
    assert_eq!(results[1].value, CellValue::Number(31.));
    assert_eq!(next.usage().lookup_prefix_scans, 2);
    drop(next);
    // Removing the overlapping axes must also clear their old membership flags.
    overlay.set_value(at("A50"), 8.).unwrap();
    assert_eq!(value(&mut overlay, "E1", &context), CellValue::Number(100.));
}

#[test]
fn retained_lookup_indices_revalidate_smaller_materialization_limits() {
    let book = indexed_lookup_book();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
            arrays: crate::eval::ArrayResultPolicy::ScalarIntersection,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    value(&mut overlay, "E1", &context);
    overlay.set_value(at("D1"), 100.).unwrap();
    let small = EvaluationContext {
        limits: crate::eval::EvaluationLimits::default().max_collection_values(10),
        ..Default::default()
    };
    let mut next = overlay.evaluation(&small).unwrap();
    // Oversized retained indices are discarded under the smaller cap. Literal
    // scanning itself needs no per-cell memo allocation, so the fallback fits.
    assert_eq!(next.cell(at("E1")).unwrap(), CellValue::Number(1000.));
    assert!(next.usage().lookup_prefix_scans > 0);
    assert_eq!(next.usage().lookup_prefix_hits, 0);
}

#[test]
fn failed_output_batch_does_not_publish_lookup_indices() {
    let mut book = indexed_lookup_book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("F1").address, "REPT(\"x\",100)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
            arrays: crate::eval::ArrayResultPolicy::ScalarIntersection,
            ..Default::default()
        })
        .build()
        .unwrap();
    let limited = EvaluationContext {
        limits: crate::eval::EvaluationLimits::default().max_text_bytes(50),
        ..Default::default()
    };
    assert!(
        overlay
            .evaluation(&limited)
            .unwrap()
            .outputs(&[at("E1"), at("F1")])
            .is_err()
    );
    let context = EvaluationContext::default();
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("E1")).unwrap(), CellValue::Number(1500.));
    assert_eq!(next.usage().lookup_prefix_scans, 150);
    assert_eq!(next.usage().lookup_prefix_hits, 0);
}

#[test]
fn lookup_indices_expire_for_context_sensitive_formula_candidates() {
    let mut book = indexed_lookup_book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("A50").address, "IF(NOW()>0,100,50)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
            arrays: crate::eval::ArrayResultPolicy::ScalarIntersection,
            ..Default::default()
        })
        .build()
        .unwrap();
    let mut context = EvaluationContext::default();
    value(&mut overlay, "E1", &context);
    overlay.set_value(at("D1"), 100.).unwrap();
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("E1")).unwrap(), CellValue::Number(500.));
    assert_eq!(next.usage().lookup_prefix_scans, 0);
    assert_eq!(next.usage().lookup_prefix_hits, 1);
    drop(next);
    context.as_of_time += Duration::from_secs(1);
    let mut changed = overlay.evaluation(&context).unwrap();
    assert_eq!(changed.cell(at("E1")).unwrap(), CellValue::Number(500.));
    // The clock consumer executes, but its stable result now cuts off the lookup.
    assert_eq!(changed.usage().formula_executions, 1);
    assert_eq!(changed.usage().lookup_prefix_scans, 0);
    assert_eq!(changed.usage().lookup_prefix_hits, 0);
}

#[test]
fn horizontal_lookup_indices_preserve_untouched_suffixes_and_reset_on_snapshot() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for col in 0..100 {
        sheet
            .set_value(CellAddress { row: 0, col }, f64::from(col + 1))
            .unwrap();
        sheet
            .set_value(CellAddress { row: 1, col }, f64::from((col + 1) * 10))
            .unwrap();
    }
    sheet.set_value(at("D4").address, 50.).unwrap();
    sheet
        .set_formula(at("E4").address, "HLOOKUP(D4,A1:CV2,2,FALSE)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
            arrays: crate::eval::ArrayResultPolicy::ScalarIntersection,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(value(&mut overlay, "E4", &context), CellValue::Number(500.));
    // An unconsumed suffix remains outside the retained prefix.
    overlay.set_value(at("CV1"), 3.).unwrap();
    overlay.set_value(at("D4"), 25.).unwrap();
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("E4")).unwrap(), CellValue::Number(250.));
    assert_eq!(next.usage().lookup_prefix_scans, 0);
    assert_eq!(next.usage().lookup_prefix_hits, 1);
    drop(next);
    let snapshot = overlay.snapshot().into_owned().unwrap();
    let (restored_workbook, restored_state) = Overlay::from_snapshot(snapshot).unwrap();
    let mut restored = restored_state.attach(&restored_workbook).unwrap();
    restored.set_value(at("D4"), 24.).unwrap();
    let mut next = restored.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("E4")).unwrap(), CellValue::Number(240.));
    assert_eq!(next.usage().lookup_prefix_scans, 24);
    drop(next);
    overlay.set_value(at("C1"), 25.).unwrap();
    assert_eq!(value(&mut overlay, "E4", &context), CellValue::Number(30.));
}

#[test]
fn lookup_index_invalidation_does_not_require_retained_formula_results() {
    let book = indexed_lookup_book();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
            arrays: crate::eval::ArrayResultPolicy::ScalarIntersection,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    value(&mut overlay, "E1", &context);
    overlay.cache.get_mut().remove((0, at("E1").address));
    assert!(overlay.cache.get_mut().entries.is_empty());
    overlay.set_value(at("A50"), 150.).unwrap();
    assert_eq!(value(&mut overlay, "E1", &context), CellValue::Number(500.));
    overlay.reset();
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("E1")).unwrap(), CellValue::Number(1500.));
    assert_eq!(next.usage().lookup_prefix_scans, 150);
}

#[test]
fn numeric_locale_invalidates_only_cells_that_consumed_numeric_text() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet.set_formula(at("A1").address, "\"1,234\"*1").unwrap();
    sheet
        .set_formula(at("B1").address, "ABS(\"1,234\")")
        .unwrap();
    sheet
        .set_formula(at("C1").address, "VALUE(\"1,234\")")
        .unwrap();
    sheet.set_formula(at("D1").address, "1+2").unwrap();
    sheet
        .set_formula(at("E1").address, "IF(TRUE,7,VALUE(\"1,234\"))")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    let mut context = EvaluationContext::default();
    let targets = [at("A1"), at("B1"), at("C1"), at("D1"), at("E1")];
    {
        let mut evaluation = overlay.evaluation(&context).unwrap();
        let outputs = evaluation.outputs(&targets).unwrap();
        assert_eq!(
            outputs.iter().map(|r| r.value.clone()).collect::<Vec<_>>(),
            vec![CellValue::Number(1234.); 3]
                .into_iter()
                .chain([CellValue::Number(3.), CellValue::Number(7.)])
                .collect::<Vec<_>>()
        );
    }
    context.number_locale =
        crate::eval::NumberLocale::new(",", Some("."), Some("EUR"), "%", ":").unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    let outputs = evaluation.outputs(&targets).unwrap();
    assert_eq!(
        outputs
            .iter()
            .take(3)
            .map(|r| r.value.clone())
            .collect::<Vec<_>>(),
        vec![CellValue::Number(1.234); 3]
    );
    assert_eq!(evaluation.usage().formula_executions, 3);
    assert_eq!(evaluation.usage().external_cache_hits, 2);
}

#[test]
fn numeric_locale_identity_and_consumption_survive_retained_snapshot() {
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("S")
        .unwrap()
        .set_formula(at("A1").address, "VALUE(\"1,234\")")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    let mut context = EvaluationContext::default();
    assert_eq!(
        value(&mut overlay, "A1", &context),
        CellValue::Number(1234.)
    );
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (restored_workbook, restored_state) =
        Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = restored_state.attach(&restored_workbook).unwrap();
    {
        let mut evaluation = restored.evaluation(&context).unwrap();
        assert_eq!(evaluation.cell(at("A1")).unwrap(), CellValue::Number(1234.));
        assert_eq!(evaluation.usage().formula_executions, 0);
    }
    context.number_locale =
        crate::eval::NumberLocale::new(",", Some("."), Some("EUR"), "%", ":").unwrap();
    let mut evaluation = restored.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("A1")).unwrap(), CellValue::Number(1.234));
    assert_eq!(evaluation.usage().formula_executions, 1);
}

#[test]
fn raw_snapshot_restore_rejects_previous_execution_semantics_versions() {
    let workbook = book();
    let overlay = Overlay::with_workbook(&workbook).build().unwrap();
    for previous in 8..76 {
        let mut bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
        // Payload's first field is its small postcard varint version.
        let version = bytes.first_mut().unwrap();
        assert_eq!(*version, 76);
        *version = previous;
        let snapshot = postcard::from_bytes(&bytes).unwrap();
        assert!(
            matches!(Overlay::from_snapshot(snapshot), Err(OverlayError::Snapshot(detail)) if detail == "unsupported snapshot version")
        );
    }
}

#[test]
fn numbervalue_locale_defaults_invalidate_only_consumers_after_snapshot_restore() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    for (cell, formula) in [
        ("A1", r#"NUMBERVALUE("1,234")"#),
        ("B1", r#"NUMBERVALUE("1,234",".",",")"#),
        ("C1", "A1*2"),
    ] {
        sheet.set_formula(at(cell).address, formula).unwrap();
    }
    let mut overlay = Overlay::with_workbook(&workbook)
        .with_policy(EvaluationPolicy {
            builtin: crate::eval::BuiltinSemanticsPolicy::ExcelDocumentation,
            ..Default::default()
        })
        .build()
        .unwrap();
    let targets = [at("A1"), at("B1"), at("C1")];
    let mut context = EvaluationContext::default();
    {
        let mut evaluation = overlay.evaluation(&context).unwrap();
        assert_eq!(
            evaluation
                .outputs(&targets)
                .unwrap()
                .iter()
                .map(|o| o.value.clone())
                .collect::<Vec<_>>(),
            vec![
                CellValue::Number(1234.),
                CellValue::Number(1234.),
                CellValue::Number(2468.)
            ]
        );
    }
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (workbook, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach(&workbook).unwrap();
    {
        let mut evaluation = restored.evaluation(&context).unwrap();
        evaluation.outputs(&targets).unwrap();
        assert_eq!(evaluation.usage().formula_executions, 0);
    }
    context.number_locale = crate::eval::NumberLocale::new(",", Some("."), None, "%", ":").unwrap();
    let mut evaluation = restored.evaluation(&context).unwrap();
    assert_eq!(
        evaluation
            .outputs(&targets)
            .unwrap()
            .iter()
            .map(|o| o.value.clone())
            .collect::<Vec<_>>(),
        vec![
            CellValue::Number(1.234),
            CellValue::Number(1234.),
            CellValue::Number(2.468)
        ]
    );
    assert_eq!(evaluation.usage().formula_executions, 2);
}

#[test]
fn approximate_match_policy_switch_and_snapshot_restore_recompute_typed_candidates() {
    let mut workbook = book();
    for (cell, formula) in [
        ("E1", "MATCH(5,{TRUE},-1)"),
        ("F1", "MATCH(12,{5;15;18;18;11;1;15;17})"),
    ] {
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula(at(cell).address, formula)
            .unwrap();
    }
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&workbook)
        .with_policy(EvaluationPolicy {
            builtin: crate::eval::BuiltinSemanticsPolicy::PinnedPoi,
            ..Default::default()
        })
        .build()
        .unwrap();
    assert_eq!(value(&mut overlay, "E1", &context), CellValue::Number(1.));
    assert_eq!(value(&mut overlay, "F1", &context), CellValue::Number(6.));
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (workbook, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut overlay = state.attach(&workbook).unwrap();
    let mut policy = overlay.policy().clone();
    policy.builtin = crate::eval::BuiltinSemanticsPolicy::ExcelDocumentation;
    overlay.set_policy(policy.clone()).unwrap();
    assert_eq!(
        value(&mut overlay, "E1", &context),
        CellValue::Error(crate::model::CellError::NA)
    );
    assert_eq!(value(&mut overlay, "F1", &context), CellValue::Number(1.));
    policy.builtin = crate::eval::BuiltinSemanticsPolicy::PinnedPoi;
    overlay.set_policy(policy).unwrap();
    assert_eq!(value(&mut overlay, "E1", &context), CellValue::Number(1.));
    assert_eq!(value(&mut overlay, "F1", &context), CellValue::Number(6.));
}

#[test]
fn roman_locale_and_arabic_reference_edits_invalidate_after_snapshot_restore() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet.set_value(at("A1").address, "X").unwrap();
    sheet.set_formula(at("B1").address, "ARABIC(A1)").unwrap();
    sheet.set_formula(at("C1").address, "ROMAN(999)").unwrap();
    sheet
        .set_formula(at("D1").address, r#"ROMAN("1,234",0)"#)
        .unwrap();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    let mut context = EvaluationContext::default();
    assert_eq!(value(&mut overlay, "B1", &context), CellValue::Number(10.));
    assert_eq!(
        value(&mut overlay, "C1", &context),
        CellValue::Text("CMXCIX".into())
    );
    assert_eq!(
        value(&mut overlay, "D1", &context),
        CellValue::Text("MCCXXXIV".into())
    );
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (workbook, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut overlay = state.attach(&workbook).unwrap();
    let mut warm = overlay.evaluation(&context).unwrap();
    assert_eq!(warm.cell(at("B1")).unwrap(), CellValue::Number(10.));
    assert_eq!(warm.usage().formula_executions, 0);
    drop(warm);
    let unrelated = overlay.cached_value(at("C1")).unwrap();
    overlay.set_value(at("A1"), "xiv").unwrap();
    assert!(overlay.cached_value(at("B1")).is_none());
    assert_eq!(value(&mut overlay, "B1", &context), CellValue::Number(14.));
    assert_eq!(
        value(&mut overlay, "C1", &context),
        CellValue::Text("CMXCIX".into())
    );
    context.number_locale = crate::eval::NumberLocale::new(",", Some("."), None, "%", ":").unwrap();
    assert_eq!(
        value(&mut overlay, "D1", &context),
        CellValue::Text("I".into())
    );
    assert_eq!(value(&mut overlay, "B1", &context), CellValue::Number(14.));
    assert!(Arc::ptr_eq(
        &unrelated,
        &overlay.cached_value(at("C1")).unwrap()
    ));
}

#[test]
// Both starts exceed their text domain under the documented Excel rules;
// policy-label changes must still invalidate and safely restore cached errors.
fn search_start_policy_switch_invalidates_restored_values_for_both_text_routes() {
    for source in [
        crate::eval::TextSourceVersion::Poi7321209,
        crate::eval::TextSourceVersion::Poi7321209OpenJdk21Unicode15,
    ] {
        let mut workbook = book();
        for (cell, formula) in [("E1", r#"SEARCH("","")"#), ("F1", r#"FIND("","abc",4)"#)] {
            workbook
                .sheet_mut(0)
                .unwrap()
                .set_formula(at(cell).address, formula)
                .unwrap();
        }
        let context = EvaluationContext::default();
        let mut overlay = Overlay::with_workbook(&workbook)
            .with_policy(EvaluationPolicy {
                text: crate::eval::TextSemanticsPolicy { source },
                builtin: crate::eval::BuiltinSemanticsPolicy::PinnedPoi,
                ..Default::default()
            })
            .build()
            .unwrap();
        assert_eq!(
            value(&mut overlay, "E1", &context),
            CellValue::Error(crate::model::CellError::Value)
        );
        assert_eq!(
            value(&mut overlay, "F1", &context),
            CellValue::Error(crate::model::CellError::Value)
        );
        let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
        let (workbook, state) =
            Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
        let mut overlay = state.attach(&workbook).unwrap();
        let mut policy = overlay.policy().clone();
        policy.builtin = crate::eval::BuiltinSemanticsPolicy::ExcelDocumentation;
        overlay.set_policy(policy.clone()).unwrap();
        assert!(overlay.cached_value(at("E1")).is_none());
        for cell in ["E1", "F1"] {
            assert_eq!(
                value(&mut overlay, cell, &context),
                CellValue::Error(crate::model::CellError::Value)
            );
        }
        policy.builtin = crate::eval::BuiltinSemanticsPolicy::PinnedPoi;
        overlay.set_policy(policy).unwrap();
        assert!(overlay.cached_value(at("F1")).is_none());
        assert_eq!(
            value(&mut overlay, "E1", &context),
            CellValue::Error(crate::model::CellError::Value)
        );
        assert_eq!(
            value(&mut overlay, "F1", &context),
            CellValue::Error(crate::model::CellError::Value)
        );
    }
}

#[test]
fn retained_scalar_named_producer_does_not_block_child_discovery_and_invalidates_before_array_change()
 {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet.set_value(at("C1").address, false).unwrap();
    sheet.set_formula(at("A1").address, "Choice").unwrap();
    workbook
        .define_name("Choice", "IF(C1,{10,20},10)", None)
        .unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(at("B2").address, "SUM(B1)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(workbook)
        .with_policy(dynamic())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(value(&mut overlay, "A1", &context), CellValue::Number(10.));
    overlay.flush().unwrap();
    let mut warm = overlay.evaluation(&context).unwrap();
    assert_eq!(warm.cell(at("B1")).unwrap(), CellValue::Blank);
    assert_eq!(warm.usage().formula_executions, 0);
    drop(warm);
    assert_eq!(value(&mut overlay, "B2", &context), CellValue::Number(0.));
    overlay.set_value(at("D5"), 99.).unwrap();
    overlay.flush().unwrap();
    let mut unrelated = overlay.evaluation(&context).unwrap();
    assert_eq!(unrelated.cell(at("B2")).unwrap(), CellValue::Number(0.));
    assert_eq!(unrelated.usage().formula_executions, 0);
    drop(unrelated);
    overlay.set_value(at("C1"), true).unwrap();
    assert!(overlay.cached_value(at("A1")).is_none());
    assert!(overlay.cached_value(at("B2")).is_none());
    // The invalidated scalar producer is recomputed with its actual new footprint.
    assert_eq!(value(&mut overlay, "B1", &context), CellValue::Number(20.));
}
#[test]
fn same_session_scalar_named_producer_tracks_future_footprint_dependency() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet.set_value(at("C1").address, false).unwrap();
    sheet.set_formula(at("A1").address, "Choice").unwrap();
    sheet.set_formula(at("B2").address, "SUM(B1)").unwrap();
    workbook
        .define_name("Choice", "IF(C1,{10,20},10)", None)
        .unwrap();
    let mut overlay = Overlay::with_workbook(workbook)
        .with_policy(dynamic())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let outputs = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&[at("A1"), at("B2")])
        .unwrap();
    assert_eq!(outputs[1].value, CellValue::Number(0.));
    assert!(overlay.cached_value(at("B2")).is_some());
    overlay.set_value(at("C1"), true).unwrap();
    assert!(overlay.cached_value(at("B2")).is_none());
    assert_eq!(value(&mut overlay, "B1", &context), CellValue::Number(20.));
}

#[test]
fn shared_provenance_survives_cache_style_and_detaches_on_same_formula_edit() {
    let mut book = book();
    let shared = crate::model::SharedFormula {
        group: crate::model::SharedFormulaId(2),
        origin: at("B1").address,
    };
    book.sheet_mut(0)
        .unwrap()
        .get_mut(at("B1").address)
        .unwrap()
        .shared_formula = Some(shared);
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    overlay
        .set_style(
            at("B1"),
            crate::model::CellStyle {
                number_format: Some("0.00".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        value(&mut overlay, "B1", &EvaluationContext::default()),
        CellValue::Number(4.)
    );
    let changes = overlay.changes();
    let mut published = book.clone();
    published.apply(changes).unwrap();
    assert_eq!(
        published
            .sheet(0)
            .unwrap()
            .get(at("B1").address)
            .unwrap()
            .shared_formula,
        Some(shared)
    );
    overlay.set_formula(at("B1"), "A1*2").unwrap();
    published.apply(overlay.changes()).unwrap();
    assert_eq!(
        published
            .sheet(0)
            .unwrap()
            .get(at("B1").address)
            .unwrap()
            .shared_formula,
        None
    );
}
