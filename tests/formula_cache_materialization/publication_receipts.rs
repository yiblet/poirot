use super::*;

#[test]
fn actual_nonincremental_publication_retains_raw_receipts_and_mutates_only_formula_caches() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at(0, 0), CellValue::Number(f64::INFINITY))
        .unwrap();
    s.set_value(
        at(1, 0),
        CellValue::Number(f64::from_bits(0xfff8_0000_0000_0099)),
    )
    .unwrap();
    s.set_value(at(2, 0), CellValue::Number(-0.0)).unwrap();
    s.set_value(
        at(3, 0),
        CellValue::Number(f64::from_bits(0x8000_0000_0000_0001)),
    )
    .unwrap();
    for row in 0..4 {
        set_formula(&mut b, row, 1, &format!("A{}", row + 1));
    }
    let c = config();
    let receipt =
        match eval::run_recalculation_with_receipts(&mut b, &c, eval::EvaluationBudgets::default())
        {
            EvaluationOutcome::Complete { value, .. } => value,
            other => panic!("{other:?}"),
        };
    assert_eq!(receipt.writes.len(), 4);

    assert_eq!(bits(get(&b, 0, 1)), f64::INFINITY.to_bits());
    assert_eq!(bits(get(&b, 1, 1)), 0xfff8_0000_0000_0099);
    assert_eq!(bits(get(&b, 2, 1)), (-0.0f64).to_bits());
    assert_eq!(bits(get(&b, 3, 1)), 0);
    assert_eq!(bits(get(&b, 1, 0)), 0xfff8_0000_0000_0099);
    assert_eq!(bits(get(&b, 3, 0)), 0x8000_0000_0000_0001);
    let nan = receipt
        .writes
        .iter()
        .find(|r| r.target.address == at(1, 1))
        .unwrap();
    assert_eq!(
        nan.value,
        S::Number(model::IeeeNumberBits::from_bits(0xfff8_0000_0000_0099))
    );
    let raw = config();
    assert!(matches!(
        eval::run_recalculation_with_configuration(
            &mut b,
            &raw,
            eval::EvaluationBudgets::default()
        ),
        EvaluationOutcome::Complete { .. }
    ));
    assert_eq!(bits(get(&b, 0, 1)), f64::INFINITY.to_bits());
    assert_eq!(bits(get(&b, 1, 1)), 0xfff8_0000_0000_0099);
}
#[test]
fn receipt_allocation_is_admitted_after_computation_and_before_any_write() {
    let mut b = book();
    set_formula(&mut b, 0, 0, "1+1");
    let before = b.clone();
    let cfg = config();
    let work = match eval::run_recalculation_with_configuration(
        &mut b.clone(),
        &cfg,
        Default::default(),
    ) {
        EvaluationOutcome::Complete { usage, .. } => usage.work_units,
        other => panic!("{other:?}"),
    };
    let budget = eval::EvaluationBudgets {
        run_work: work,
        ..Default::default()
    };
    match eval::run_recalculation_with_receipts(&mut b, &cfg, budget) {
        EvaluationOutcome::Aborted {
            failure: eval::EvaluationError::Budget(failure),
            usage,
        } => {
            assert_eq!(failure.kind, eval::BudgetKind::RunWork);
            assert_eq!(failure.requested, 1);
            assert_eq!(usage.work_units, work);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(&b, &before);
    assert!(matches!(
        eval::run_recalculation_with_configuration(&mut b, &cfg, budget),
        EvaluationOutcome::Complete { .. }
    ));
    assert_eq!(get(&b, 0, 0), CellValue::Number(2.));
}
