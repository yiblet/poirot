use super::*;
fn book() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("S").unwrap();
    w
}
fn at(col: u32) -> CellAddress {
    CellAddress { row: 0, col }
}
fn configured(policy: NumericOutcomePolicy) -> RunConfiguration {
    RunConfiguration::default().with_numeric_policy(policy)
}
fn run(w: &Workbook, formula: &str, policy: NumericOutcomePolicy) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        w,
        0,
        at(25),
        formula,
        &configured(policy),
        EvaluationBudgets::default(),
    )
}
fn complete(outcome: EvaluationOutcome<CellValue>) -> CellValue {
    match outcome {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
fn bits(value: CellValue) -> u64 {
    match value {
        Number(n) => n.to_bits(),
        other => panic!("{other:?}"),
    }
}
#[test]
fn configured_all_inverse_spellings_consume_raw_source_result_contract() {
    let mut w = book();
    w.sheet_mut(0).unwrap().set_value(at(0), 1e-300).unwrap();
    for formula in [
        "NORMSINV(1E-300)",
        "NORM.S.INV(1E-300)",
        "NORMINV(1E-300,0,1)",
        "NORM.INV(1E-300,0,1)",
        "NORMSINV(A1)",
        "NORM.S.INV(A1)",
        "NORMINV(A1,0,1)",
        "NORM.INV(A1,0,1)",
    ] {
        assert_eq!(
            complete(run(&w, formula, NumericOutcomePolicy::LegacyFinite)),
            Error(Num),
            "{formula}"
        );
        assert_eq!(
            bits(complete(run(
                &w,
                formula,
                NumericOutcomePolicy::PinnedPoi7321209
            ))),
            f64::NEG_INFINITY.to_bits(),
            "{formula}"
        );
    }
    for p in [
        NumericOutcomePolicy::LegacyFinite,
        NumericOutcomePolicy::PinnedPoi7321209,
    ] {
        for f in [
            "NORMSINV(0)",
            "NORM.S.INV(1)",
            "NORMINV(0,0,1)",
            "NORM.INV(1,0,1)",
        ] {
            assert_eq!(complete(run(&w, f, p)), Error(Num));
        }
        assert_eq!(
            bits(complete(run(&w, "NORMSINV(0.5)", p))),
            0.0f64.to_bits()
        );
    }
}
#[test]
fn actual_consumers_distinguish_cell_errors_source_numbers_and_source_exceptions() {
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_formula(at(0), "NORMSINV(1E-300)", Blank)
        .unwrap();
    let p = NumericOutcomePolicy::PinnedPoi7321209;
    for f in ["A1", "IFERROR(A1,42)", "IFERROR(NORMSINV(1E-300),42)"] {
        assert_eq!(bits(complete(run(&w, f, p))), f64::NEG_INFINITY.to_bits());
    }
    assert_eq!(complete(run(&w, "ISNUMBER(A1)", p)), Boolean(true));
    for f in ["SUM(A1)", "SUM(NORMSINV(1E-300))", "ABS(A1)"] {
        assert_eq!(complete(run(&w, f, p)), Error(Num));
    }
    assert_eq!(complete(run(&w, "IFERROR(SUM(A1),42)", p)), Number(42.0));
    assert_eq!(complete(run(&w, "SUM(A1,#DIV/0!)", p)), Error(Div0));
    for f in ["A1<0", "IFERROR(A1<0,42)"] {
        assert!(
            matches!(run(&w,f,p),EvaluationOutcome::Aborted {failure:EvaluationFailure::KnownSourceException {
            exception: KnownSourceException::SpecialDoubleComparison { argument:0,bits },.. },.. } if bits.bits()==f64::NEG_INFINITY.to_bits())
        );
    }
    assert_eq!(complete(run(&w, "A1<\"text\"", p)), Boolean(true));
    assert_eq!(complete(run(&w, "\"text\">A1", p)), Boolean(true));
}
#[test]
fn construction_stages_flush_both_subnormal_signs_preserve_existing_negative_zero() {
    let p = NumericOutcomePolicy::PinnedPoi7321209;
    for raw in [1, 0x8000_0000_0000_0001] {
        let mut w = book();
        w.sheet_mut(0)
            .unwrap()
            .set_value(at(0), Number(f64::from_bits(raw)))
            .unwrap();
        assert_eq!(bits(complete(run(&w, "A1", p))), 0);
        assert_eq!(
            bits(complete(run(&w, "A1", NumericOutcomePolicy::LegacyFinite))),
            raw
        );
        assert_eq!(complete(run(&w, "NORMSINV(A1)", p)), Error(Num));
    }
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(0), Number(-0.0))
        .unwrap();
    assert_eq!(bits(complete(run(&w, "A1", p))), (-0.0f64).to_bits());
    assert_eq!(bits(complete(run(&w, "-A1", p))), 0);
    assert_eq!(bits(complete(run(&w, "A1*1", p))), 0);
    assert_eq!(bits(complete(run(&w, "A1-0", p))), (-0.0f64).to_bits());
    assert_eq!(complete(run(&w, "A1<0", p)), Boolean(true));
    assert_eq!(bits(complete(run(&w, "2.2250738585072014E-308/2", p))), 0);
    assert_eq!(bits(complete(run(&w, "2.2250738585072014E-308%", p))), 0);
    assert_eq!(
        complete(run(&w, "NORMSINV(4.9406564584124654E-324)", p)),
        Error(Num)
    );
    for mean in ["4.9406564584124654E-324", "-4.9406564584124654E-324"] {
        let f = format!("NORM.INV(0.5,\"{mean}\",1)");
        assert_eq!(
            bits(complete(run(&w, &f, p))),
            0,
            "function-result NumberEval flushes either sign"
        );
    }
    // NumberPtg token gate differs from a stored numeric-cell read.
    let mut session = Session::new(
        &w,
        EvaluationInputs::default(),
        BudgetMeter::new(EvaluationBudgets::default()).unwrap(),
    );
    session.numeric_policy = p;
    let admitted = session
        .expr(0, at(1), &Expr::Number(f64::INFINITY))
        .unwrap();
    assert_eq!(session.scalar(at(1), admitted).unwrap(), Error(Num));
    assert_eq!(
        bits(p.at(
            numeric_outcome::NumericConstruction::NumberEvalResult,
            f64::from_bits(1)
        )),
        0
    );
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(0), Number(f64::INFINITY))
        .unwrap();
    assert_eq!(bits(complete(run(&w, "A1", p))), f64::INFINITY.to_bits());
    assert_eq!(
        complete(run(&w, "A1", NumericOutcomePolicy::LegacyFinite)),
        Error(Num)
    );
}
#[test]
fn configured_source_constructor_exception_is_explicit() {
    let w = book();
    let p = NumericOutcomePolicy::PinnedPoi7321209;
    assert!(matches!(
        run(&w, "IFERROR(POISSON(1,0,FALSE),42)", p),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::KnownSourceException {
                exception: KnownSourceException::PoissonMeanZeroConstructor,
                ..
            },
            ..
        }
    ));
}
#[test]
fn retained_overlay_reuses_infinity_and_invalidates_numeric_profile() {
    use crate::overlay::{EvaluationContext, Overlay, OverlayPolicy};
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_formula(at(0), "NORMSINV(1E-300)", Blank)
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_formula(at(1), "2+3", Blank)
        .unwrap();
    let mut overlay = Overlay::with_workbook(&w)
        .with_policy(OverlayPolicy {
            numeric: NumericOutcomePolicy::PinnedPoi7321209,
            ..Default::default()
        })
        .build()
        .unwrap();
    let target = overlay.target("S!A1").unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        bits(overlay.evaluation(&context).unwrap().cell(target).unwrap()),
        f64::NEG_INFINITY.to_bits()
    );
    let raw = overlay.cached_value(target).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(
        bits(evaluation.cell(target).unwrap()),
        f64::NEG_INFINITY.to_bits()
    );
    assert_eq!(evaluation.usage().formula_executions, 0);
    drop(evaluation);
    assert!(std::sync::Arc::ptr_eq(
        &raw,
        &overlay.cached_value(target).unwrap()
    ));
    let mut policy = overlay.policy().clone();
    policy.numeric = NumericOutcomePolicy::LegacyFinite;
    overlay.set_policy(policy).unwrap();
    assert!(overlay.cached_value(target).is_none());
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(target).unwrap(),
        Error(Num)
    );
    let legacy = configured(NumericOutcomePolicy::LegacyFinite);
    let text_only = legacy
        .clone()
        .with_text_policy(poirot_text::TextSemanticsPolicy {
            source: poirot_text::SourceVersion::Poi7321209,
        });
    assert_eq!(
        text_only.numeric_policy(),
        NumericOutcomePolicy::LegacyFinite
    );
    assert_eq!(
        complete(run_formula_with_configuration(
            &w,
            0,
            at(25),
            "NORMSINV(1E-300)",
            &text_only,
            Default::default()
        )),
        Error(Num)
    );
}
#[test]
fn retained_overlay_normalization_keeps_raw_source_dependency_identity() {
    use crate::overlay::{EvaluationContext, Overlay, OverlayPolicy};
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(0), Number(f64::from_bits(1)))
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_formula(at(1), "A1", Blank)
        .unwrap();
    let mut overlay = Overlay::with_workbook(&w)
        .with_policy(OverlayPolicy {
            numeric: NumericOutcomePolicy::PinnedPoi7321209,
            ..Default::default()
        })
        .build()
        .unwrap();
    let input = overlay.target("S!A1").unwrap();
    let output = overlay.target("S!B1").unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        bits(overlay.evaluation(&context).unwrap().cell(output).unwrap()),
        0
    );
    for raw in [2, 0, 0x8000_0000_0000_0000] {
        overlay
            .set_value(input, Number(f64::from_bits(raw)))
            .unwrap();
        assert!(overlay.cached_value(output).is_none());
        let mut evaluation = overlay.evaluation(&context).unwrap();
        let expected = if raw == 0x8000_0000_0000_0000 { raw } else { 0 };
        assert_eq!(bits(evaluation.cell(output).unwrap()), expected);
        assert_eq!(evaluation.usage().formula_executions, 1);
    }
}
