use super::*;
fn configuration(source: TextSourceVersion) -> RunConfiguration {
    RunConfiguration::default().with_text_policy(TextSemanticsPolicy { source })
}
#[test]
fn explicit_java_profile_alone_admits_plain_lone_surrogates() {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    let lone = CellValue::from(poirot_model::ExcelText::from_units(vec![0xd800]));
    let sheet = w.sheet_mut(0).unwrap();
    for cell in ["A1", "D1"] {
        sheet
            .set_value(cell.parse().unwrap(), lone.clone())
            .unwrap();
    }
    sheet.set_value("B1".parse().unwrap(), 42.).unwrap();
    sheet.set_value("A2".parse().unwrap(), 42.).unwrap();
    for (formula, expected) in [
        ("IFERROR(MATCH(D1,A1:A1,0),99)", Number(1.)),
        ("IFERROR(VLOOKUP(D1,A1:B1,2,FALSE),99)", Number(42.)),
        ("IFERROR(HLOOKUP(D1,A1:A2,2,FALSE),99)", Number(42.)),
    ] {
        for source in [
            TextSourceVersion::PoirotF6c572d,
            TextSourceVersion::Poi7321209OpenJdk21Unicode15,
        ] {
            let outcome = run_formula_with_configuration(
                &w,
                0,
                "Z20".parse().unwrap(),
                formula,
                &configuration(source),
                EvaluationBudgets::default(),
            );
            if source == TextSourceVersion::PoirotF6c572d {
                assert!(matches!(
                    outcome,
                    EvaluationOutcome::Aborted {
                        failure: EvaluationFailure::UnsupportedCapability { .. },
                        ..
                    }
                ));
            } else {
                assert!(
                    matches!(outcome, EvaluationOutcome::Complete { value, .. } if value == expected)
                );
            }
        }
    }
}
fn asset_refusal() {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    let outcome = run_formula_with_configuration(
        &w,
        0,
        "Z20".parse().unwrap(),
        "IFERROR(MATCH(\"I\",{\"i\"},0),99)",
        &configuration(TextSourceVersion::Poi7321209OpenJdk21Unicode15),
        EvaluationBudgets {
            run_work: 5999,
            ..EvaluationBudgets::default()
        },
    );
    assert!(matches!(
        outcome,
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
}
#[test]
fn java_cold_asset_refusal_escapes_iferror() {
    // Run this test alone in a fresh process to exercise uninitialized mapping tables:
    // cargo test -p poirot-eval java_cold_asset_refusal_escapes_iferror -- --test-threads=1
    asset_refusal();
}
#[test]
fn java_explicitly_warm_asset_refusal_escapes_iferror() {
    poirot_functions_text::java_compare::admit().unwrap();
    asset_refusal();
}
#[test]
fn numeric_lookup_has_no_java_mapping_admission() {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    let run = |source| {
        run_formula_with_configuration(
            &w,
            0,
            "Z20".parse().unwrap(),
            "MATCH(2,{1;2;3},0)",
            &configuration(source),
            EvaluationBudgets {
                run_work: 1000,
                ..EvaluationBudgets::default()
            },
        )
    };
    let legacy = run(TextSourceVersion::PoirotF6c572d);
    let java = run(TextSourceVersion::Poi7321209OpenJdk21Unicode15);
    assert_eq!(legacy.usage(), java.usage());
    assert!(matches!(
        java,
        EvaluationOutcome::Complete {
            value: Number(2.),
            ..
        }
    ));
}

fn historical_legacy_compare(
    key: &CellValue,
    value: &CellValue,
    mode: SearchMode,
    is_match: bool,
    budget: &mut BudgetMeter,
) -> FunctionResult<Option<Ordering>> {
    budget.charge(WorkKind::Comparison, 1)?;
    let zero = Number(0.);
    let key = if *key == Blank { &zero } else { key };
    let same = (key.text_ref().is_some() && value.text_ref().is_some())
        || matches!(
            (key, value),
            (Number(_), Number(_)) | (Boolean(_), Boolean(_))
        );
    if !same {
        return Ok(None);
    }
    if let (Some(pattern), Some(candidate)) = (key.text_ref(), value.text_ref()) {
        // MATCH uses wildcard comparison in all modes; V/H lookup uses it only in exact mode.
        if is_match || mode == SearchMode::Exact {
            let pattern = checked_text_view(pattern, budget)?;
            if pattern.contains(['*', '?']) {
                let candidate = checked_text_view(candidate, budget)?;
                return Ok(Some(if wildcard(&pattern, &candidate, budget)? {
                    Ordering::Equal
                } else {
                    Ordering::Less
                }));
            }
        }
    }
    compare(key, value, budget).map(Some)
}

#[test]
fn legacy_comparison_preserves_pre146_result_and_exact_admission_effects() {
    let values = [
        Text("plain".into()),
        Text("PLAIN".into()),
        Text("p*".into()),
        Text("p?ain".into()),
        CellValue::from(poirot_model::ExcelText::from_units(vec![0xd800])),
        Number(1.),
        Blank,
    ];
    for key in &values {
        for value in &values {
            for mode in [
                SearchMode::Exact,
                SearchMode::Ascending,
                SearchMode::Descending,
            ] {
                for is_match in [false, true] {
                    for work in [1, 8, 10000] {
                        let limits = EvaluationBudgets {
                            run_work: work,
                            ..Default::default()
                        };
                        let mut old = BudgetMeter::new(limits).unwrap();
                        let mut current = BudgetMeter::new(limits).unwrap();
                        let expected =
                            historical_legacy_compare(key, value, mode, is_match, &mut old);
                        let actual = lookup_compare(
                            key,
                            value,
                            mode,
                            is_match,
                            TextSourceVersion::PoirotF6c572d,
                            &mut current,
                        );
                        assert_eq!(actual, expected);
                        assert_eq!(current.usage, old.usage);
                    }
                }
            }
        }
    }
}
