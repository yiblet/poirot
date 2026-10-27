//! Incremental reuse follows consumed value order and preserves ordinary lazy execution.
use crate::eval::config::EvaluationContext;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::functions;
use crate::eval::overlay::incremental::Overlay;
use crate::model::{CellError, CellValue, Workbook};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

fn workbook(formulas: &[(&str, &str)]) -> Workbook {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet.set_value(at("A1").address, 1.1).unwrap();
    sheet.set_value(at("A2").address, 2.1).unwrap();
    for &(address, formula) in formulas {
        sheet.set_formula(at(address).address, formula).unwrap();
    }
    workbook
}

fn numeric_graph() -> Workbook {
    workbook(&[
        ("B1", "ROUND(A1,0)"),
        ("B2", "ROUND(A2,0)"),
        ("C1", "B1+B2"),
        ("C2", "B1*B2"),
        ("D1", "C1*2"),
        ("D2", "C1+C2"),
    ])
}

#[test]
fn multiple_dirty_dependencies_cut_off_nested_diamond_and_keep_original_edges() {
    let workbook = numeric_graph();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    let context = EvaluationContext::default();
    let outputs = [at("D1"), at("D2")];
    overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&outputs)
        .unwrap();
    // ROUND(1.1,0)=ROUND(1.2,0)=1; ROUND(2.1,0)=ROUND(2.2,0)=2.
    for (a, b) in [(1.2, 2.2), (1.3, 2.3)] {
        overlay.set_value(at("A1"), a).unwrap();
        overlay.set_value(at("A2"), b).unwrap();
        let mut evaluation = overlay.evaluation(&context).unwrap();
        let values = evaluation.outputs(&outputs).unwrap();
        assert_eq!(values[0].value, CellValue::Number(6.));
        assert_eq!(values[1].value, CellValue::Number(5.));
        assert_eq!(
            evaluation.usage().formula_executions,
            2,
            "only the two rounded inputs execute, not any parent"
        );
    }
    // Reusing parents must preserve both original edges for a subsequent real change.
    overlay.set_value(at("A1"), 1.4).unwrap();
    overlay.set_value(at("A2"), 2.8).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    let values = evaluation.outputs(&outputs).unwrap();
    assert_eq!(values[0].value, CellValue::Number(8.));
    assert_eq!(values[1].value, CellValue::Number(7.));
    assert_eq!(evaluation.usage().formula_executions, 6);
}

#[test]
fn range_consumers_can_validate_multiple_unchanged_values() {
    let workbook = workbook(&[
        ("B1", "ROUND(A1,0)"),
        ("B2", "ROUND(A2,0)"),
        ("C1", "SUM(B1:B2)"),
    ]);
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Number(3.)
    );
    overlay.set_value(at("A1"), 1.2).unwrap();
    overlay.set_value(at("A2"), 2.2).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("C1")).unwrap(), CellValue::Number(3.));
    assert_eq!(evaluation.usage().formula_executions, 2);
}

#[test]
fn changed_condition_avoids_operational_failure_in_obsolete_if_and_iferror_branches() {
    for (condition, parent, initial, changed, expected) in [
        ("A1>0", "IF(Z1,LEN(B2),7)", 1., -1., 7.),
        ("IF(A1>0,1,1/0)", "IFERROR(Z1,LEN(B2))", -1., 1., 1.),
    ] {
        let workbook = workbook(&[("Z1", condition), ("B2", "REPT(\"x\",A2)"), ("C1", parent)]);
        let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
        let context = EvaluationContext::default();
        overlay.set_value(at("A1"), initial).unwrap();
        overlay.set_value(at("A2"), 1.).unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("C1"))
                .unwrap(),
            CellValue::Number(1.)
        );
        overlay.set_value(at("A1"), changed).unwrap();
        overlay.set_value(at("A2"), 1000.).unwrap();
        let mut constrained = context.clone();
        constrained.limits = constrained.limits.max_text_bytes(64);
        let mut evaluation = overlay.evaluation(&constrained).unwrap();
        assert_eq!(
            evaluation.cell(at("C1")).unwrap(),
            CellValue::Number(expected)
        );
        assert_eq!(
            evaluation.usage().formula_executions,
            2,
            "the obsolete REPT dependency must never execute"
        );
    }
}

#[test]
fn direct_edits_and_parent_context_changes_force_execution() {
    let workbook = workbook(&[
        ("B1", "ROUND(A1,0)"),
        ("B2", "ROUND(A2,0)"),
        ("C1", "B1+B2+NOW()"),
    ]);
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    let mut context = EvaluationContext::default();
    let first = overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("C1"))
        .unwrap();
    overlay.set_value(at("A1"), 1.2).unwrap();
    overlay.set_value(at("A2"), 2.2).unwrap();
    context.as_of_time += Duration::from_secs(86400);
    let mut evaluation = overlay.evaluation(&context).unwrap();
    let changed = evaluation.cell(at("C1")).unwrap();
    assert_eq!(evaluation.usage().formula_executions, 3);
    assert_ne!(first, changed);
    drop(evaluation);
    overlay.set_formula(at("C1"), "B1+B2+10").unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("C1")).unwrap(), CellValue::Number(13.));
    assert_eq!(evaluation.usage().formula_executions, 1);
    drop(evaluation);
    overlay.set_value(at("B1"), 9.).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("C1")).unwrap(), CellValue::Number(21.));
    assert_eq!(evaluation.usage().formula_executions, 1);
}

#[test]
fn volatile_and_opaque_parents_cannot_hide_behind_equal_dependencies() {
    use crate::eval::functions::extensions::*;
    for cacheability in [
        ExtensionCacheability::Volatile,
        ExtensionCacheability::Opaque,
    ] {
        let calls = Arc::new(AtomicUsize::new(0));
        let captured = Arc::clone(&calls);
        let mut registry = ExtensionRegistry::new();
        registry
            .register(
                ExtensionNamespace::UserDefined,
                "TICK",
                "conditional-test/v1",
                ExtensionContract {
                    arity: functions::registry::Arity {
                        minimum: 0,
                        maximum: 0,
                    },
                    evaluation: ExtensionEvaluationStrategy::Eager,
                    capabilities: ExtensionCapabilities {
                        references: false,
                        arrays: false,
                        context: false,
                    },
                    cacheability,
                },
                move |_host: &mut dyn ExtensionHost| {
                    captured.fetch_add(1, Ordering::Relaxed);
                    Ok(ExtensionOutput::Scalar(CellValue::Number(10.)))
                },
            )
            .unwrap();
        let workbook = workbook(&[
            ("B1", "ROUND(A1,0)"),
            ("B2", "ROUND(A2,0)"),
            ("C1", "B1+B2+TICK()"),
        ]);
        let mut overlay = Overlay::with_workbook(&workbook)
            .with_extensions(&registry)
            .build()
            .unwrap();
        let context = EvaluationContext::default();
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap();
        overlay.set_value(at("A1"), 1.2).unwrap();
        overlay.set_value(at("A2"), 2.2).unwrap();
        let mut evaluation = overlay.evaluation(&context).unwrap();
        assert_eq!(evaluation.cell(at("C1")).unwrap(), CellValue::Number(13.));
        assert_eq!(evaluation.usage().formula_executions, 3);
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }
}

#[test]
fn validation_abort_publishes_nothing_and_fresh_cycle_can_retry() {
    let workbook = numeric_graph();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("D2"))
        .unwrap();
    overlay.set_value(at("A1"), 1.2).unwrap();
    overlay.set_value(at("A2"), 2.2).unwrap();
    assert!(overlay.cached_value(at("D2")).is_none());
    let mut constrained = context.clone();
    constrained.limits = constrained.limits.max_work(1);
    let mut evaluation = overlay.evaluation(&constrained).unwrap();
    assert!(evaluation.cell(at("D2")).is_err());
    assert!(evaluation.result_changes().unwrap().is_empty());
    drop(evaluation);
    assert!(overlay.cached_value(at("D2")).is_none());
    let mut constrained = context.clone();
    constrained.limits = constrained.limits.max_depth(1);
    assert!(
        overlay
            .evaluation(&constrained)
            .unwrap()
            .cell(at("D2"))
            .is_err()
    );
    assert!(overlay.cached_value(at("D2")).is_none());
    overlay.set_formula(at("E1"), "REPT(\"x\",1000)").unwrap();
    let mut constrained = context.clone();
    constrained.limits = constrained.limits.max_text_bytes(64);
    let mut failed = overlay.evaluation(&constrained).unwrap();
    assert!(failed.outputs(&[at("D2"), at("E1")]).is_err());
    assert!(failed.result_changes().unwrap().is_empty());
    drop(failed);
    assert!(overlay.cached_value(at("D2")).is_none());
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("D2")).unwrap(), CellValue::Number(5.));
    assert_eq!(evaluation.usage().formula_executions, 2);
    drop(evaluation);
    overlay.set_formula(at("B1"), "C1").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        CellValue::Error(CellError::Ref)
    );
}

#[test]
fn snapshots_keep_ordered_traces_and_omit_unvalidated_results() {
    let workbook = numeric_graph();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("D2"))
        .unwrap();
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (restored_workbook, state) =
        Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach(&restored_workbook).unwrap();
    restored.set_value(at("A1"), 1.2).unwrap();
    restored.set_value(at("A2"), 2.2).unwrap();
    let dirty = postcard::to_allocvec(&restored.snapshot()).unwrap();
    let (dirty_workbook, state) =
        Overlay::from_snapshot(postcard::from_bytes(&dirty).unwrap()).unwrap();
    let dirty = state.attach(&dirty_workbook).unwrap();
    assert!(dirty.cached_value(at("D2")).is_none());
    let mut evaluation = restored.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("D2")).unwrap(), CellValue::Number(5.));
    assert_eq!(evaluation.usage().formula_executions, 2);
}
