use super::*;
use std::time::Duration;
fn at(reference: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: reference.parse().unwrap(),
    }
}
fn book() -> Workbook {
    let mut book = Workbook::new();
    let s = book.add_sheet("S").unwrap();
    s.set_value(at("A1").address, 2.).unwrap();
    s.set_formula(at("B1").address, "A1*2", CellValue::Blank)
        .unwrap();
    s.set_formula(at("C1").address, "B1+1", CellValue::Blank)
        .unwrap();
    s.set_formula(at("D1").address, "7*3", CellValue::Blank)
        .unwrap();
    book
}
fn value<W: Borrow<Workbook>>(
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
    assert_eq!(overlay.compiled_usage().parse_attempts, 2);
    let mut next = overlay.evaluation(&context).unwrap();
    assert_eq!(next.cell(at("C1")).unwrap(), CellValue::Number(5.));
    assert_eq!(next.usage().formula_executions, 0);
    assert_eq!(next.usage().external_cache_hits, 1);
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
        .set_formula(at("E1").address, "SUM(A2:A10)", CellValue::Blank)
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
    s.set_formula(at("D2").address, "IF(A2,B2,C2)", CellValue::Blank)
        .unwrap();
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
fn volatile_cells_and_dependents_run_again_but_unrelated_cells_reuse() {
    let mut book = book();
    let s = book.sheet_mut(0).unwrap();
    s.set_formula(at("E1").address, "RAND()", CellValue::Blank)
        .unwrap();
    s.set_formula(at("F1").address, "E1+1", CellValue::Blank)
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&[at("F1"), at("D1")])
        .unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    evaluation.outputs(&[at("F1"), at("D1")]).unwrap();
    assert_eq!(evaluation.usage().formula_executions, 2);
    assert_eq!(evaluation.usage().external_cache_hits, 1);
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
        .set_formula(at("E1").address, "REPT(\"x\",100)", CellValue::Blank)
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext {
        budgets: crate::EvaluationBudgets {
            text_bytes: 30,
            ..Default::default()
        },
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
        .set_formula(at("E1").address, "1+", CellValue::Blank)
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    overlay.compile_all();
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let snapshot = postcard::from_bytes(&bytes).unwrap();
    let (overlay_workbook, overlay_state) = Overlay::from_snapshot(snapshot).unwrap();
    let mut overlay = overlay_state.attach(&overlay_workbook).unwrap();
    assert_eq!(
        value(&mut overlay, "E1", &EvaluationContext::default()),
        CellValue::Error(poirot_model::CellError::Value)
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

fn dynamic() -> OverlayPolicy {
    OverlayPolicy {
        builtin: crate::BuiltinSemanticsPolicy::ExcelDocumentation,
        arrays: crate::ArrayResultPolicy::DynamicSpill,
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
        CellValue::Error(poirot_model::CellError::Spill)
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
    s.set_formula(at("E1").address, "SUM(A1:A3 A2:B2)", CellValue::Blank)
        .unwrap();
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
    s.set_formula(at("E2").address, "@A1:A3", CellValue::Blank)
        .unwrap();
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
fn publication_policy_reuses_raw_results_and_changes_only_saved_values() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("E1").address, "NORMSINV(1E-300)", CellValue::Blank)
        .unwrap();
    let policy = OverlayPolicy {
        numeric: crate::NumericOutcomePolicy::PinnedPoi7321209,
        ..Default::default()
    };
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy)
        .build()
        .unwrap();
    assert_eq!(
        value(&mut overlay, "E1", &context),
        CellValue::Number(f64::NEG_INFINITY)
    );
    let original = overlay.cached_value(at("E1")).unwrap();
    let mut policy = overlay.policy().clone();
    policy.materialization = crate::FormulaCacheMaterializationPolicy::PinnedPoiCellSetter;
    overlay.set_policy(policy).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(
        evaluation.cell(at("E1")).unwrap(),
        CellValue::Number(f64::NEG_INFINITY)
    );
    assert_eq!(evaluation.usage().formula_executions, 0);
    assert!(
        evaluation
            .result_changes()
            .unwrap()
            .iter()
            .any(|c| matches!(
                c,
                WorkbookChange::SetCachedValue {
                    value: CellValue::Error(poirot_model::CellError::Div0),
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
            CellValue::TextUtf16(poirot_model::ExcelText::from("A")),
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
        .set_formula(at("C1").address, "MATCH(100,A1:A200,0)", CellValue::Blank)
        .unwrap();
    sheet
        .set_formula(at("C2").address, "MATCH(50,A1:A200,0)", CellValue::Blank)
        .unwrap();
    sheet
        .set_formula(at("D2").address, "C2*2", CellValue::Blank)
        .unwrap();
    sheet
        .set_formula(at("C3").address, "MATCH(300,A1:A200,0)", CellValue::Blank)
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
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
        .set_formula(at("E1").address, "REPT(\"x\",100)", CellValue::Blank)
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext {
        budgets: crate::EvaluationBudgets {
            text_bytes: 50,
            ..Default::default()
        },
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
    use crate::extensions::*;
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
                arity: crate::registry::Arity {
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
        .set_formula(at("E1").address, "USER_VALUE()", CellValue::Blank)
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
    sheet
        .set_formula(at("C1").address, "A2+B1+A3", CellValue::Blank)
        .unwrap();
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
fn volatile_root_index_tracks_branch_replacement_and_snapshot_restore() {
    let mut book = book();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_value(at("A2").address, false).unwrap();
    sheet
        .set_formula(at("E1").address, "IF(A2,RAND(),7)", CellValue::Blank)
        .unwrap();
    sheet
        .set_formula(at("F1").address, "E1+1", CellValue::Blank)
        .unwrap();
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
        assert_eq!(evaluation.usage().formula_executions, 2);
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
