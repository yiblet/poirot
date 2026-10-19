use poirot_eval::{
    EvaluationBudgets, EvaluationFailure, EvaluationOutcome, RunConfiguration, TextSemanticsPolicy,
    TextSourceVersion, run_formula_with_configuration,
};
use poirot_model::{CellAddress, CellValue, Workbook};

fn run(formula: &str, configuration: &RunConfiguration) -> EvaluationOutcome<CellValue> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    run_formula_with_configuration(
        &workbook,
        0,
        CellAddress { row: 0, col: 0 },
        formula,
        configuration,
        EvaluationBudgets::default(),
    )
}

#[test]
fn public_configuration_selects_documented_pairs_or_utf16_len() {
    let legacy = RunConfiguration::default().with_text_policy(TextSemanticsPolicy {
        source: TextSourceVersion::ExcelCompatibilityV2Documented,
    });
    assert_eq!(
        legacy.text_policy().source,
        TextSourceVersion::ExcelCompatibilityV2Documented
    );
    let poi = legacy.clone().with_text_policy(TextSemanticsPolicy {
        source: TextSourceVersion::Poi7321209,
    });
    for (configuration, expected) in [(&legacy, 1.), (&poi, 2.)] {
        assert!(matches!(run("LEN(\"😀\")", configuration),
            EvaluationOutcome::Complete { value: CellValue::Number(value), .. } if value == expected));
    }
    assert_ne!(legacy.fingerprint(), poi.fingerprint());
}

#[test]
fn configured_poi_boundaries_remain_operational() {
    let poi = RunConfiguration::default().with_text_policy(TextSemanticsPolicy {
        source: TextSourceVersion::Poi7321209,
    });
    for formula in ["MID(\"abc\",2,2147483647)", "LOWER(\"Ä\")", "UPPER(\"ß\")"] {
        assert!(
            matches!(
                run(formula, &poi),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::UnsupportedCapability { .. },
                    ..
                }
            ),
            "{formula}"
        );
    }
}

#[test]
fn poi_concat_has_no_invented_global_argument_cap() {
    let poi = RunConfiguration::default().with_text_policy(TextSemanticsPolicy {
        source: TextSourceVersion::Poi7321209,
    });
    let arguments = vec!["\"x\""; 256].join(",");
    for name in ["CONCAT", "CONCATENATE"] {
        let formula = format!("{name}({arguments})");
        assert!(
            matches!(run(&formula, &poi), EvaluationOutcome::Complete {
            value, ..
        } if value == CellValue::Text("x".repeat(256))),
            "{name}"
        );
    }
}

#[test]
fn dependency_port_records_actual_text_policy_consumption() {
    use poirot_eval::{
        DependencyRead, DependencyReadSink, EvaluationPolicy, EvaluationRun, EvaluationTarget,
        FormulaCompiler, ReadEvent, WorkbookView,
    };
    #[derive(Default)]
    /// Records actual text-policy consumption for incremental invalidation assertions.
    struct Reads(Vec<ReadEvent>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event);
        }
    }
    let target = EvaluationTarget {
        sheet: 0,
        address: CellAddress { row: 0, col: 0 },
    };
    let configuration = RunConfiguration::default();
    for (formula, consumed) in [
        ("LEN(\"😀\")", true),
        ("\"x\"&\"y\"", true),
        ("IF(FALSE,LEN(\"😀\"),7)", false),
        ("TRIM(\" x \" )", false),
        ("#N/A&\"x\"", false),
    ] {
        let mut workbook = Workbook::new();
        workbook.add_sheet("Sheet1").unwrap();
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula(target.address, formula, CellValue::Blank)
            .unwrap();
        let policy = EvaluationPolicy::default();
        let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
        let mut reads = Reads::default();
        {
            let mut run = EvaluationRun::new_with_configuration(
                WorkbookView::new(&workbook, &plan, &policy).unwrap(),
                &configuration,
                EvaluationBudgets::default(),
            )
            .unwrap()
            .with_dependency_sink(&mut reads);
            run.evaluate_cell(target).unwrap();
        }
        assert_eq!(
            reads
                .0
                .iter()
                .any(|event| event.read == DependencyRead::TextPolicy),
            consumed,
            "{formula}"
        );
    }
}

#[test]
fn code_retained_ascii_type_is_policy_specific() {
    let legacy = RunConfiguration::default();
    let poi = legacy.clone().with_text_policy(TextSemanticsPolicy {
        source: TextSourceVersion::Poi7321209,
    });
    for (configuration, expected) in [(&poi, CellValue::Text("65".into()))] {
        match run(r#"CODE("A")"#, configuration) {
            EvaluationOutcome::Complete { value, .. } => assert_eq!(value, expected),
            outcome => panic!("{outcome:?}"),
        }
        assert!(matches!(run(r#"UNICODE("A")"#, configuration),
            EvaluationOutcome::Complete { value: CellValue::Number(value), .. } if value == 65.));
    }
}

#[test]
fn poi_code_decimal_output_is_admitted_before_allocation_and_iferror_cannot_catch_budget() {
    let configuration = RunConfiguration::default().with_text_policy(TextSemanticsPolicy {
        source: TextSourceVersion::Poi7321209,
    });
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    // Preparation has its own spelling budget. Bind normally so these limits
    // exercise actual execution, including the one-byte argument admissions.
    let syntax = poirot_parsing::formula::parse_formula(r#"IFERROR(CODE("A"),7)"#).unwrap();
    let plan = poirot_eval::bound::FormulaPlan::bind(&syntax).unwrap();
    let target = poirot_eval::EvaluationTarget {
        sheet: 0,
        address: CellAddress { row: 0, col: 0 },
    };
    for limit in [1, 2] {
        let outcome = poirot_eval::run_plan_with_configuration(
            &workbook,
            target.sheet,
            target.address,
            &plan,
            &configuration,
            EvaluationBudgets {
                text_bytes: limit,
                ..EvaluationBudgets::default()
            },
        );
        assert!(outcome.usage().expressions > 0, "{outcome:?}");
        assert!(outcome.usage().text_bytes > 0, "{outcome:?}");
        if limit == 1 {
            assert!(
                matches!(outcome,
                EvaluationOutcome::Aborted { failure: EvaluationFailure::Budget(ref failure), .. }
                    if failure.kind == poirot_eval::BudgetKind::TextBytes
                        && failure.limit == 1
                        && failure.consumed == 0
                        && failure.requested == 2
                        && failure.target == Some(target)),
                "{outcome:?}"
            );
        } else {
            assert!(
                matches!(outcome,
                EvaluationOutcome::Complete { value: CellValue::Text(ref value), .. } if value == "65"),
                "{outcome:?}"
            );
        }
    }
}
