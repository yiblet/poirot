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
        let outcome = run_formula_with_configuration(
            &w,
            0,
            "Z20".parse().unwrap(),
            formula,
            &configuration(TextSourceVersion::Poi7321209OpenJdk21Unicode15),
            EvaluationBudgets::default(),
        );
        assert!(matches!(outcome, EvaluationOutcome::Complete { value, .. } if value == expected));
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
    let java = run(TextSourceVersion::Poi7321209OpenJdk21Unicode15);
    assert!(matches!(
        java,
        EvaluationOutcome::Complete {
            value: Number(2.),
            ..
        }
    ));
}
