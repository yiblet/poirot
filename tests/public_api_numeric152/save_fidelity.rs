use super::*;

fn numeric_refusal<'a>(
    error: &'a (dyn std::error::Error + 'static),
) -> Option<&'a model::NumericFidelityRefusal> {
    let mut current = Some(error);
    while let Some(e) = current {
        if let Some(n) = e.downcast_ref::<model::NumericFidelityRefusal>() {
            return Some(n);
        }
        current = e.source();
    }
    None
}
#[test]
fn save_plans_refuse_raw_nonfinite_cache_and_accept_explicit_materialization() {
    let mut source = book();
    set_formula(&mut source, 0, 0, "NORMSINV(1E-300)");
    let mut overlay = poirot::eval::overlay::Overlay::with_workbook(source.model())
        .with_policy(policy(M::PreserveComputed))
        .build()
        .unwrap();
    let context = poirot::EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(target(0, 0))
        .unwrap();
    let raw = overlay.cached_value(target(0, 0)).unwrap();
    let result = output(&mut overlay, &source);
    let error = result.save_xlsx().build().unwrap_err();
    let refusal = numeric_refusal(&error).expect("typed numeric fidelity refusal");
    assert_eq!(refusal.bits.bits(), f64::NEG_INFINITY.to_bits());
    assert!(refusal.location.formula_cache);
    overlay.set_policy(policy(M::PinnedPoiCellSetter)).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    evaluation.cell(target(0, 0)).unwrap();
    assert_eq!(evaluation.usage().formula_executions, 0);
    drop(evaluation);
    assert!(std::sync::Arc::ptr_eq(
        &raw,
        &overlay.cached_value(target(0, 0)).unwrap()
    ));
    let result = output(&mut overlay, &source);
    let bytes = result.save_xlsx().to_buffer().unwrap();
    let reopened = Workbook::from_bytes(&bytes).unwrap();
    assert_eq!(get(&reopened, 0, 0), CellValue::Error(CellError::Div0));
    assert!(
        reopened
            .model()
            .sheet(0)
            .unwrap()
            .get(at(0, 0))
            .unwrap()
            .formula
            .is_some()
    );
}

#[test]
fn nan_save_roundtrip_preserves_raw_computation_and_writes_num_errors() {
    let mut source = book();
    set_formula(
        &mut source,
        0,
        0,
        "NORMDIST(\"1.7976931348623157e308\",\"-1.7976931348623157e308\",\"1.7976931348623157e308\",TRUE)",
    );
    set_formula(&mut source, 0, 1, "IFERROR(A1,42)");
    let mut overlay = poirot::eval::overlay::Overlay::with_workbook(source.model())
        .with_policy(policy(M::PreserveComputed))
        .build()
        .unwrap();
    let context = poirot::EvaluationContext::default();
    let values = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&[target(0, 0), target(0, 1)])
        .unwrap();
    let nan = bits(values[0].value.clone());
    assert!(f64::from_bits(nan).is_nan());
    assert_eq!(bits(values[1].value.clone()), nan);
    let raw = overlay.cached_value(target(0, 0)).unwrap();
    overlay.set_policy(policy(M::PinnedPoiCellSetter)).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    evaluation.outputs(&[target(0, 0), target(0, 1)]).unwrap();
    assert_eq!(evaluation.usage().formula_executions, 0);
    drop(evaluation);
    assert!(std::sync::Arc::ptr_eq(
        &raw,
        &overlay.cached_value(target(0, 0)).unwrap()
    ));
    assert_eq!(bits((*raw).clone()), nan);
    let result = output(&mut overlay, &source);
    let bytes = result.save_xlsx().to_buffer().unwrap();
    let reopened = Workbook::from_bytes(&bytes).unwrap();
    for col in [0, 1] {
        assert_eq!(get(&reopened, 0, col), CellValue::Error(CellError::Num));
    }
}
