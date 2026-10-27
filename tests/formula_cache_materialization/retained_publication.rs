use super::*;
use poirot::calculation::EvaluationContext;
use poirot::eval::overlay::Overlay;
use std::sync::Arc;

#[test]
fn failed_batches_and_invalid_policies_preserve_previous_successful_outputs() {
    let mut source = book();
    set_formula(&mut source, 0, 0, "1+1");
    set_formula(&mut source, 0, 1, "IFERROR(A1,42)");
    let before = source.clone();
    let mut overlay = Overlay::with_workbook(&source)
        .with_policy(policy())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let targets = [target(0, 0), target(0, 1)];
    overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&targets)
        .unwrap();
    let raw = overlay.cached_value(targets[0]).unwrap();
    let committed = overlay.changes();
    let mut limited = context.clone();
    limited.limits = limited.limits.max_work(0);
    assert!(matches!(
        overlay.evaluation(&limited).unwrap().outputs(&targets),
        Err(eval::EvaluationError::Budget(_))
    ));
    assert_eq!(overlay.changes(), committed);
    assert!(Arc::ptr_eq(
        &raw,
        &overlay.cached_value(targets[0]).unwrap()
    ));
    limited = context.clone();
    limited.retention.cells = 0;
    assert!(matches!(
        overlay.evaluation(&limited),
        Err(poirot::calculation::OverlayError::Evaluation(
            eval::EvaluationError::MemoRetentionLimit { .. }
        ))
    ));
    assert_eq!(overlay.changes(), committed);
    assert_eq!(&source, &before);
    let result = output(&mut overlay, &source);
    assert_eq!(get(&result, 0, 0), CellValue::Number(2.));
}
