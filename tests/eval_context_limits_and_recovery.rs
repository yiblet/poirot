//! Quotas, cycle recovery, and consuming source recovery are evaluator API contracts.
//! Numeric expectations below follow integer arithmetic, independent of cached results.
use poirot::eval::EvaluationContext;
use poirot::eval::overlay::{Evaluation, Overlay, OverlayError};
use poirot::eval::{BudgetKind, EvaluationFailure, EvaluationLimits, EvaluationTarget};
use poirot::model::{CellValue, Workbook};
use std::borrow::Borrow;
use std::time::UNIX_EPOCH;

fn target(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

fn source() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    sheet.set_value(target("A1").address, 3.).unwrap();
    sheet
        .set_formula_with_cached_value(target("B1").address, "A1*2", 99.0.into())
        .unwrap();
    book
}

fn context(limits: EvaluationLimits) -> EvaluationContext {
    EvaluationContext {
        as_of_time: UNIX_EPOCH,
        limits,
        ..Default::default()
    }
}

#[test]
fn one_limits_field_preserves_absent_zero_and_explicit_maximum_quotas() {
    let book = source();
    for limits in [
        EvaluationLimits::default(),
        EvaluationLimits::default()
            .max_work(u64::MAX)
            .max_formula_work(u64::MAX)
            .max_collection_values(usize::MAX)
            .max_text_bytes(usize::MAX)
            .max_wildcard_states(u64::MAX)
            .max_depth(128),
    ] {
        let expected = limits;
        let mut calculation = Evaluation::with_workbook(&book)
            .build_calculation(context(limits))
            .unwrap();
        assert_eq!(calculation.context().limits, expected);
        assert_eq!(calculation.cell(target("B1")).unwrap(), 6.0.into());
    }
    let mut configured = context(EvaluationLimits::default().max_work(u64::MAX));
    assert_eq!(configured.limits.work_limit(), Some(u64::MAX));
    configured.limits = configured.limits.without_work_limit();
    assert_eq!(configured.limits.work_limit(), None);
    for (limits, kind) in [
        (EvaluationLimits::default().max_work(0), BudgetKind::RunWork),
        (
            EvaluationLimits::default().max_formula_work(0),
            BudgetKind::CellWork,
        ),
        (EvaluationLimits::default().max_depth(0), BudgetKind::Depth),
    ] {
        let mut calculation = Evaluation::with_workbook(&book)
            .build_calculation(context(limits))
            .unwrap();
        assert!(
            matches!(calculation.cell(target("B1")), Err(EvaluationFailure::Budget(f)) if f.kind == kind)
        );
    }
    let invalid = Evaluation::with_workbook(&book)
        .build_calculation(context(EvaluationLimits::default().max_depth(129)));
    assert!(
        matches!(invalid, Err(OverlayError::Evaluation(EvaluationFailure::Budget(f))) if f.kind == BudgetKind::Configuration)
    );
}

#[test]
fn refusal_is_sticky_until_an_explicit_direct_or_overlay_cycle() {
    let book = source();
    let limited = context(EvaluationLimits::default().max_work(0));
    let fresh = context(EvaluationLimits::default());
    let mut direct = Evaluation::with_workbook(&book)
        .build_calculation(limited.clone())
        .unwrap();
    direct.set_value(target("A1"), 4.).unwrap();
    let refused = direct.cell(target("B1")).unwrap_err();
    direct.clear_cache();
    assert_eq!(direct.cell(target("B1")).unwrap_err(), refused);
    direct.start_cycle(fresh.clone()).unwrap();
    assert_eq!(direct.cell(target("B1")).unwrap(), 8.0.into());

    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    overlay.set_value(target("A1"), 5.).unwrap();
    {
        let mut cycle = overlay.calculation(&limited).unwrap();
        let refused = cycle.cell(target("B1")).unwrap_err();
        assert_eq!(cycle.cell(target("B1")).unwrap_err(), refused);
    }
    assert_eq!(
        overlay
            .calculation(&fresh)
            .unwrap()
            .cell(target("B1"))
            .unwrap(),
        10.0.into()
    );
    assert_eq!(overlay.cached_cells(), 1);
    {
        let cycle = overlay.calculation(&limited).unwrap();
        drop(cycle);
    }
    // Work-only changes retain valid computation identity. Admission still runs on hits.
    assert_eq!(overlay.cached_cells(), 1);
    assert!(
        overlay
            .calculation(&limited)
            .unwrap()
            .cell(target("B1"))
            .is_err()
    );
    assert_eq!(overlay.cached_cells(), 1);
}

/// Owned source handle deliberately without Clone, proving extraction moves its owner.
struct OwnedSource(Workbook);
impl Borrow<Workbook> for OwnedSource {
    fn borrow(&self) -> &Workbook {
        &self.0
    }
}

#[test]
fn consuming_recovery_moves_original_source_without_publishing_scenario_state() {
    let original = OwnedSource(source());
    let allocation = original.0.sheet(0).unwrap().name().as_ptr();
    let mut direct = Evaluation::with_workbook(original)
        .build_calculation(context(EvaluationLimits::default()))
        .unwrap();
    direct.set_value(target("A1"), 9.).unwrap();
    assert_eq!(direct.cell(target("B1")).unwrap(), 18.0.into());
    let recovered = direct.into_workbook();
    assert_eq!(recovered.0.sheet(0).unwrap().name().as_ptr(), allocation);
    assert_eq!(
        recovered
            .0
            .sheet(0)
            .unwrap()
            .get(target("A1").address)
            .unwrap()
            .value,
        3.0.into()
    );
    assert_eq!(
        recovered
            .0
            .sheet(0)
            .unwrap()
            .get(target("B1").address)
            .unwrap()
            .value,
        99.0.into()
    );

    let mut overlay = Overlay::with_workbook(recovered).build().unwrap();
    overlay.set_value(target("A1"), 10.).unwrap();
    assert_eq!(
        overlay
            .calculation(&context(EvaluationLimits::default()))
            .unwrap()
            .cell(target("B1"))
            .unwrap(),
        20.0.into()
    );
    let recovered = overlay.into_workbook();
    assert_eq!(recovered.0.sheet(0).unwrap().name().as_ptr(), allocation);
    assert_eq!(
        recovered
            .0
            .sheet(0)
            .unwrap()
            .get(target("A1").address)
            .unwrap()
            .value,
        CellValue::Number(3.)
    );
    assert_eq!(
        recovered
            .0
            .sheet(0)
            .unwrap()
            .get(target("B1").address)
            .unwrap()
            .value,
        CellValue::Number(99.)
    );
}

#[test]
fn owned_overlay_recovery_preserves_explicitly_flushed_edits_only() {
    let mut overlay = Overlay::with_workbook(source()).build().unwrap();
    overlay.set_value(target("A1"), 4.).unwrap();
    assert_eq!(overlay.apply_edits_to_workbook().unwrap(), 1);
    overlay.set_value(target("A1"), 5.).unwrap();
    let book = overlay.into_workbook();
    assert_eq!(
        book.sheet(0)
            .unwrap()
            .get(target("A1").address)
            .unwrap()
            .value,
        4.0.into()
    );
    assert_eq!(
        book.sheet(0)
            .unwrap()
            .get(target("B1").address)
            .unwrap()
            .value,
        99.0.into()
    );
}
