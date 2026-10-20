//! Canonical overlay checks for consumed sparse lookup absence.
use super::*;
use crate::overlay::{EvaluationContext, Overlay};
#[test]
fn matching_insertions_removals_and_unrelated_edits_validate_consumed_guards() {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1")
        .unwrap()
        .set_formula_with_cached_value(
            "Z1".parse().unwrap(),
            "IFERROR(VLOOKUP(7,A:B,2,FALSE),99)",
            Number(-999.),
        )
        .unwrap();
    let mut overlay = Overlay::with_workbook(&w).build().unwrap();
    let target = overlay.target("Sheet1!Z1").unwrap();
    let context = EvaluationContext::default();
    let run = |overlay: &mut Overlay<'_>, expected, executions| {
        let mut evaluation = overlay.evaluation(&context).unwrap();
        assert_eq!(evaluation.cell(target).unwrap(), expected);
        assert_eq!(evaluation.usage().formula_executions, executions);
    };
    run(&mut overlay, Number(99.), 1);
    run(&mut overlay, Number(99.), 0);
    let c = overlay.target("Sheet1!C500").unwrap();
    overlay.set_value(c, 7.).unwrap();
    run(&mut overlay, Number(99.), 0);
    let a = overlay.target("Sheet1!A500").unwrap();
    let b = overlay.target("Sheet1!B500").unwrap();
    overlay.set_value(a, 7.).unwrap();
    overlay.set_value(b, 42.).unwrap();
    run(&mut overlay, Number(42.), 1);
    // An unconsumed suffix cannot affect the exact first match.
    let later = overlay.target("Sheet1!A1000").unwrap();
    overlay.set_value(later, 7.).unwrap();
    run(&mut overlay, Number(42.), 0);
    overlay.remove_cell(a).unwrap();
    run(&mut overlay, Number(0.), 1);
    overlay.remove_cell(later).unwrap();
    run(&mut overlay, Number(99.), 1);
}
