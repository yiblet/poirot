use super::*;
#[test]
fn formula_sheet_and_scoped_names_share_model_policy() {
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Ä Data")
        .unwrap()
        .set_value("A1".parse().unwrap(), 7.)
        .unwrap();
    workbook.add_sheet("Other").unwrap();
    workbook.define_name("ÉRate", "1", None).unwrap();
    workbook.define_name("érate", "2", Some(0)).unwrap();
    for (sheet, formula, expected) in [(0, "'ä dATA'!A1", 7.), (0, "éRATE", 2.), (1, "éRATE", 1.)]
    {
        assert_eq!(
            match crate::eval::run_formula(
                &workbook,
                sheet,
                "B1".parse().unwrap(),
                formula,
                Default::default(),
                Default::default()
            ) {
                crate::eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            Number(expected)
        );
    }
    workbook.define_name("ÄLoop", "äloop", None).unwrap();
    assert_eq!(
        match crate::eval::run_formula(
            &workbook,
            0,
            "B1".parse().unwrap(),
            "ÄLOOP",
            Default::default(),
            Default::default()
        ) {
            crate::eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        Error(Ref)
    );
}
#[test]
fn unresolved_identifiers_are_operational_diagnostics() {
    let mut workbook = Workbook::new();
    workbook.import_sheet("Σ").unwrap();
    workbook.named_ranges.push(crate::model::NamedRange {
        name: "Σ".into(),
        formula: "1".into(),
        sheet_scope: None,
    });
    for formula in ["'σ'!A1", "σ"] {
        assert!(matches!(
            run_formula(
                &workbook,
                0,
                "B1".parse().unwrap(),
                formula,
                EvaluationInputs::default(),
                EvaluationBudgets::default()
            ),
            EvaluationOutcome::Aborted {
                failure: EvaluationFailure::UnsupportedCapability {
                    capability: CapabilityId::IdentifierComparison,
                    ..
                },
                ..
            }
        ));
    }
}

#[test]
fn distinct_utf16_lengths_allow_sheet_creation_and_runtime_lookup() {
    for (stored, requested, formula) in [("Σ", "Étage", "'étage'!A1"), ("😀", "a", "'A'!A1")] {
        let mut workbook = Workbook::new();
        workbook.add_sheet(stored).unwrap();
        workbook
            .add_sheet(requested)
            .unwrap()
            .set_value("A1".parse().unwrap(), 7.)
            .unwrap();
        assert_eq!(
            match crate::eval::run_formula(
                &workbook,
                0,
                "B1".parse().unwrap(),
                formula,
                Default::default(),
                Default::default()
            ) {
                crate::eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            Number(7.)
        );
    }
}

#[test]
fn equal_utf16_astral_lengths_keep_runtime_lookup_diagnostic() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("😀").unwrap();
    assert!(matches!(
        run_formula(
            &workbook,
            0,
            "B1".parse().unwrap(),
            "'ab'!A1",
            EvaluationInputs::default(),
            EvaluationBudgets::default()
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::UnsupportedCapability {
                capability: CapabilityId::IdentifierComparison,
                ..
            },
            ..
        }
    ));
}
