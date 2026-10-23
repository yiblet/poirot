use super::*;
use poirot::{EvaluationContext, Overlay};
use std::sync::Arc;

#[test]
fn materialization_switch_reuses_raw_infinity_and_preserves_dependents() {
    let mut source = book();
    set_formula(&mut source, 0, 0, "NORMSINV(1E-300)");
    set_formula(&mut source, 0, 1, "IFERROR(A1,42)");
    set_formula(&mut source, 0, 2, "ISNUMBER(A1)");
    let mut overlay = Overlay::with_workbook(source.model())
        .with_policy(policy(M::PreserveComputed))
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let targets = [target(0, 0), target(0, 1), target(0, 2)];
    overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&targets)
        .unwrap();
    let raw = overlay.cached_value(targets[0]).unwrap();
    let dependent = overlay.cached_value(targets[1]).unwrap();
    for materialization in [M::PinnedPoiCellSetter, M::PreserveComputed] {
        overlay.set_policy(policy(materialization)).unwrap();
        let mut evaluation = overlay.evaluation(&context).unwrap();
        let values = evaluation.outputs(&targets).unwrap();
        assert_eq!(bits(values[0].value.clone()), f64::NEG_INFINITY.to_bits());
        assert_eq!(bits(values[1].value.clone()), f64::NEG_INFINITY.to_bits());
        assert_eq!(values[2].value, CellValue::Boolean(true));
        assert_eq!(evaluation.usage().formula_executions, 0);
        drop(evaluation);
        assert!(Arc::ptr_eq(
            &raw,
            &overlay.cached_value(targets[0]).unwrap()
        ));
        assert!(Arc::ptr_eq(
            &dependent,
            &overlay.cached_value(targets[1]).unwrap()
        ));
        let result = output(&mut overlay, &source);
        for col in [0, 1] {
            if materialization == M::PinnedPoiCellSetter {
                assert_eq!(get(&result, 0, col), CellValue::Error(CellError::Div0));
            } else {
                assert_eq!(bits(get(&result, 0, col)), f64::NEG_INFINITY.to_bits());
            }
        }
        assert_eq!(get(&result, 0, 2), CellValue::Boolean(true));
    }
}

#[test]
fn density_and_nan_results_survive_reuse_and_snapshot_without_becoming_errors() {
    let mut source = book();
    source
        .model_mut()
        .sheet_mut(0)
        .unwrap()
        .set_value(at(0, 3), f64::MAX)
        .unwrap();
    set_formula(&mut source, 0, 0, "NORM.DIST(0,0,\"5e-324\",FALSE)");
    set_formula(&mut source, 1, 0, "NORMDIST(D1,-D1,D1,TRUE)");
    for row in 0..2 {
        set_formula(&mut source, row, 1, &format!("IFERROR(A{},42)", row + 1));
        set_formula(&mut source, row, 2, &format!("ISNUMBER(A{})", row + 1));
    }
    let mut overlay = Overlay::with_workbook(source.model())
        .with_policy(policy(M::PreserveComputed))
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let targets = (0..2)
        .flat_map(|row| (0..3).map(move |col| target(row, col)))
        .collect::<Vec<_>>();
    let values = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&targets)
        .unwrap();
    assert_eq!(bits(values[0].value.clone()), f64::INFINITY.to_bits());
    let nan = bits(values[3].value.clone());
    assert!(f64::from_bits(nan).is_nan());
    assert_eq!(bits(values[4].value.clone()), nan);
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (restored, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut overlay = state.attach(&restored).unwrap();
    overlay.set_policy(policy(M::PinnedPoiCellSetter)).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    let values = evaluation.outputs(&targets).unwrap();
    assert_eq!(evaluation.usage().formula_executions, 0);
    assert_eq!(bits(values[3].value.clone()), nan);
    drop(evaluation);
    let result = output(&mut overlay, &Workbook::from_model(restored.clone()));
    for (row, error) in [(0, CellError::Div0), (1, CellError::Num)] {
        assert_eq!(get(&result, row, 0), CellValue::Error(error));
        assert_eq!(get(&result, row, 1), CellValue::Error(error));
        assert_eq!(get(&result, row, 2), CellValue::Boolean(true));
    }
}

#[test]
fn failed_batches_and_invalid_policies_preserve_previous_successful_outputs() {
    let mut source = book();
    set_formula(&mut source, 0, 0, "NORMSINV(1E-300)");
    set_formula(&mut source, 0, 1, "IFERROR(A1,42)");
    let before = source.model().clone();
    let mut overlay = Overlay::with_workbook(source.model())
        .with_policy(policy(M::PinnedPoiCellSetter))
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
    limited.budgets.run_work = 0;
    assert!(matches!(
        overlay.evaluation(&limited).unwrap().outputs(&targets),
        Err(eval::EvaluationFailure::Budget(_))
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
        Err(poirot::OverlayError::Evaluation(
            eval::EvaluationFailure::MemoRetentionLimit { .. }
        ))
    ));
    assert_eq!(overlay.changes(), committed);
    let failure = overlay.target("S!C1").unwrap();
    overlay.set_formula(failure, "POISSON(1,0,FALSE)").unwrap();
    assert!(matches!(
        overlay
            .evaluation(&context)
            .unwrap()
            .outputs(&[targets[0], failure]),
        Err(eval::EvaluationFailure::KnownSourceException { .. })
    ));
    assert!(overlay.cached_value(failure).is_none());
    assert!(Arc::ptr_eq(
        &raw,
        &overlay.cached_value(targets[0]).unwrap()
    ));
    assert_eq!(source.model(), &before);
    let result = output(&mut overlay, &source);
    assert_eq!(get(&result, 0, 0), CellValue::Error(CellError::Div0));
}

#[test]
fn mixed_cross_sheet_results_publish_only_as_a_complete_explicit_batch() {
    let mut source = book();
    source.model_mut().add_sheet("Other").unwrap();
    set_formula(&mut source, 0, 0, "NORMSINV(1E-300)");
    source
        .model_mut()
        .sheet_mut(1)
        .unwrap()
        .set_formula(at(0, 0), "S!A1")
        .unwrap();
    let mut overlay = Overlay::with_workbook(source.model())
        .with_policy(policy(M::PinnedPoiCellSetter))
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let cross = eval::EvaluationTarget {
        sheet: 1,
        address: at(0, 0),
    };
    let values = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&[target(0, 0), cross])
        .unwrap();
    assert_eq!(bits(values[1].value.clone()), f64::NEG_INFINITY.to_bits());
    let result = output(&mut overlay, &source);
    assert_eq!(
        result
            .model()
            .sheet(1)
            .unwrap()
            .get(at(0, 0))
            .unwrap()
            .value,
        CellValue::Error(CellError::Div0)
    );
    assert_eq!(
        source
            .model()
            .sheet(1)
            .unwrap()
            .get(at(0, 0))
            .unwrap()
            .value,
        CellValue::Blank
    );
}
