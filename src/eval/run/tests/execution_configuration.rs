//! Tests of the execution_configuration contract.
use super::*;

#[test]
fn formatting_is_consumed_only_when_selected_and_is_not_captured_by_plans() {
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(A1, "DOLLAR(1)")
        .unwrap();
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    let currency = crate::eval::CurrencyContext {
        decimal_separator: ".".into(),
        grouping_separator: ",".into(),
        primary_group: 3,
        secondary_group: 3,
        positive_prefix: "$".into(),
        positive_suffix: String::new(),
        negative_prefix: "($".into(),
        negative_suffix: ")".into(),
    };
    let first = crate::eval::RunConfiguration::new(
        context(),
        crate::eval::FormattingConfiguration::with_currency(currency.clone()),
    );
    let mut changed = currency;
    changed.positive_prefix = "USD ".into();
    let second = crate::eval::RunConfiguration::new(
        context(),
        crate::eval::FormattingConfiguration::with_currency(changed),
    );
    assert_ne!(first.fingerprint(), second.fingerprint());
    for (configuration, expected) in [(&first, "$1.00"), (&second, "USD 1.00")] {
        let mut reads = Reads::default();
        let run = EvaluationRun::new_with_configuration(
            WorkbookView::new(&workbook, &plan, &policy).unwrap(),
            configuration,
            EvaluationBudgets::default(),
        )
        .unwrap()
        .with_dependency_sink(&mut reads);
        assert_eq!(
            value(
                run.evaluate_probes(&[EvaluationProbe::Cell(target(A1))])
                    .outcome
            )[0]
            .value,
            CellValue::Text(expected.into())
        );
        assert!(reads.0.contains(&ReadEvent {
            consumer: Some(target(A1)),
            read: DependencyRead::Formatting
        }));
    }
    assert_eq!(compiler.usage().parse_attempts, 1);
    let missing = crate::eval::RunConfiguration::from(context());
    let mut reads = Reads::default();
    let run = EvaluationRun::new_with_configuration(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        &missing,
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_dependency_sink(&mut reads);
    assert!(matches!(
        run.evaluate_probes(&[EvaluationProbe::Cell(target(A1))])
            .outcome,
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::UnsupportedCapability {
                capability: crate::eval::CapabilityId::Formatting,
                ..
            },
            ..
        }
    ));
    assert!(reads.0.iter().any(|e| e.read == DependencyRead::Formatting));
    for (source, expected) in [
        ("IF(FALSE,DOLLAR(1),9)", CellValue::Number(9.)),
        ("DOLLAR()", CellValue::Error(CellError::Value)),
    ] {
        let prepared = compiler.compile_formula(source, &policy);
        let mut reads = Reads::default();
        let run = EvaluationRun::new_with_configuration(
            WorkbookView::new(&workbook, &plan, &policy).unwrap(),
            &missing,
            EvaluationBudgets::default(),
        )
        .unwrap()
        .with_dependency_sink(&mut reads);
        assert_eq!(
            value(
                run.evaluate_probes(&[EvaluationProbe::Formula {
                    target: target(B1),
                    formula: &prepared
                }])
                .outcome
            )[0]
            .value,
            expected
        );
        assert!(!reads.0.iter().any(|e| e.read == DependencyRead::Formatting));
    }
}

#[test]
fn provider_identity_rebinds_unknown_names_and_only_selected_calls_are_consumed() {
    use crate::eval::extensions::*;
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(B1, "IF(FALSE,CUSTOM(9),7)")
        .unwrap();
    let mut compiler = FormulaCompiler::new();
    let plain = compiler.compile_workbook(&workbook, &PreparationPolicy::default());
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "CUSTOM",
            "identity-v1",
            ExtensionContract {
                arity: crate::eval::registry::Arity {
                    minimum: 1,
                    maximum: 1,
                },
                evaluation: ExtensionEvaluationStrategy::Eager,
                capabilities: ExtensionCapabilities {
                    references: false,
                    arrays: false,
                    context: false,
                },
                cacheability: ExtensionCacheability::Volatile,
            },
            |_host: &mut dyn ExtensionHost| -> ExtensionResult<ExtensionOutput> {
                Ok(ExtensionOutput::Argument(0))
            },
        )
        .unwrap();
    let policy = PreparationPolicy::with_extensions(
        poirot_parsing::formula::FormulaParsePolicy::default(),
        &registry,
    );
    assert!(!plain.matches(&workbook, &policy));
    let plan = compiler.compile_workbook_with_extensions(&workbook, &policy, &registry);
    let mut reads = Reads::default();
    let run = EvaluationRun::new_with_extensions(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
        &registry,
    )
    .unwrap()
    .with_dependency_sink(&mut reads);
    assert_eq!(
        value(
            run.evaluate_probes(&[EvaluationProbe::Cell(target(B1))])
                .outcome
        )[0]
        .value,
        CellValue::Number(7.)
    );
    assert!(!reads.0.iter().any(|event| matches!(
        event.read,
        DependencyRead::Extension(_) | DependencyRead::OpaqueProvider(_)
    )));
    let selected = compiler.compile_formula_with_extensions("CUSTOM(9)", &policy, &registry);
    let before = compiler.usage().clone();
    let mut reads = Reads::default();
    let run = EvaluationRun::new_with_extensions(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
        &registry,
    )
    .unwrap()
    .with_dependency_sink(&mut reads);
    assert_eq!(
        value(
            run.evaluate_probes(&[EvaluationProbe::Formula {
                target: target(B1),
                formula: &selected
            }])
            .outcome
        )[0]
        .value,
        CellValue::Number(9.)
    );
    assert_eq!(compiler.usage(), &before);
    assert_eq!(
        reads
            .0
            .iter()
            .filter(|event| matches!(event.read, DependencyRead::Extension(_)))
            .count(),
        1
    );
    assert!(
        reads
            .0
            .iter()
            .all(|event| !matches!(event.read, DependencyRead::OpaqueProvider(_)))
    );
    let invalid = compiler.compile_formula("CUSTOM(9)", &policy);
    let mut run = EvaluationRun::new_with_extensions(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
        &registry,
    )
    .unwrap();
    assert_eq!(
        run.evaluate_formula(target(B1), &invalid),
        Err(EvaluationFailure::StalePlan)
    );
}

#[test]
fn random_consumption_tracks_actual_draws_and_session_memo() {
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(B1, "RAND()")
        .unwrap();
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    let mut reads = Reads::default();
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_dependency_sink(&mut reads);
    let report = run.evaluate_probes(&[
        EvaluationProbe::Cell(target(B1)),
        EvaluationProbe::Cell(target(B1)),
    ]);
    let actual = reads
        .0
        .iter()
        .filter_map(|event| match &event.read {
            DependencyRead::Random(access) => Some(access.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), 1);
    assert_eq!(actual[0].draws, 1);
    assert_eq!(actual[0].words, 1);
    assert_eq!(report.execution.session_memo_hits, 1);
    match report.outcome {
        EvaluationOutcome::Complete { usage, .. } => assert_eq!(usage.random_accesses, actual),
        other => panic!("{other:?}"),
    }
    for source in ["IF(FALSE,RAND(),7)", "RANDBETWEEN(3,2)", "RAND(1)"] {
        let prepared = compiler.compile_formula(source, &policy);
        let mut reads = Reads::default();
        let run = EvaluationRun::new(
            WorkbookView::new(&workbook, &plan, &policy).unwrap(),
            context(),
            EvaluationBudgets::default(),
        )
        .unwrap()
        .with_dependency_sink(&mut reads);
        let _ = run.evaluate_probes(&[EvaluationProbe::Formula {
            target: target(B1),
            formula: &prepared,
        }]);
        assert!(
            !reads
                .0
                .iter()
                .any(|event| matches!(event.read, DependencyRead::Random(_)))
        );
    }
}
#[test]
fn dollar_and_rand_have_independent_dependency_categories_and_consumption() {
    use crate::eval::registry::{DependencyPolicy, resolve_function};
    let dollar = resolve_function(&poirot_parsing::formula::FunctionName::from("DOLLAR"))
        .unwrap()
        .dependencies;
    let rand = resolve_function(&poirot_parsing::formula::FunctionName::from("RAND"))
        .unwrap()
        .dependencies;
    assert!(dollar.contains(DependencyPolicy::CURRENCY));
    assert!(!dollar.contains(DependencyPolicy::SEED));
    assert!(rand.contains(DependencyPolicy::SEED));
    assert!(!rand.contains(DependencyPolicy::CURRENCY));
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(A1, "DOLLAR(1)")
        .unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(B1, "RAND()")
        .unwrap();
    let configuration = crate::eval::RunConfiguration::new(
        context(),
        crate::eval::FormattingConfiguration::with_currency(crate::eval::CurrencyContext {
            decimal_separator: ".".into(),
            grouping_separator: ",".into(),
            primary_group: 3,
            secondary_group: 3,
            positive_prefix: "$".into(),
            positive_suffix: String::new(),
            negative_prefix: "($".into(),
            negative_suffix: ")".into(),
        }),
    );
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    for (address, formatting, randomness) in [(A1, true, false), (B1, false, true)] {
        let mut reads = Reads::default();
        let run = EvaluationRun::new_with_configuration(
            WorkbookView::new(&workbook, &plan, &policy).unwrap(),
            &configuration,
            EvaluationBudgets::default(),
        )
        .unwrap()
        .with_dependency_sink(&mut reads);
        let values = value(
            run.evaluate_probes(&[EvaluationProbe::Cell(target(address))])
                .outcome,
        );
        if formatting {
            assert_eq!(values[0].value, CellValue::Text("$1.00".into()));
        } else {
            assert!(
                matches!(values[0].value, CellValue::Number(number) if (0.0..1.0).contains(&number))
            );
        }
        assert_eq!(
            reads
                .0
                .iter()
                .any(|event| matches!(event.read, DependencyRead::Formatting)),
            formatting
        );
        assert_eq!(
            reads
                .0
                .iter()
                .any(|event| matches!(event.read, DependencyRead::Random(_))),
            randomness
        );
        assert!(!reads.0.iter().any(|event| matches!(event.read, DependencyRead::Context(mask) if mask.contains(if formatting { DependencyPolicy::SEED } else { DependencyPolicy::CURRENCY }))));
    }
}

#[test]
fn formatted_number_locale_is_exact_configuration_and_lazy_consumption() {
    let workbook = workbook();
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let formula = compiler.compile_formula("\"1,234\"*1", &policy);
    let configuration = crate::eval::RunConfiguration::from(context());
    let changed = configuration.clone().with_number_locale(
        crate::eval::NumberLocale::new(",", Some("."), Some("EUR"), "%", ":").unwrap(),
    );
    assert_ne!(configuration.fingerprint(), changed.fingerprint());
    for (configuration, expected) in [(&configuration, 1234.), (&changed, 1.234)] {
        let mut reads = Reads::default();
        let outcome = crate::eval::run_formula_with_configuration(
            &workbook,
            0,
            A1,
            "\"1,234\"*1",
            configuration,
            EvaluationBudgets::default(),
        );
        assert!(
            matches!(outcome, crate::eval::EvaluationOutcome::Complete { value: CellValue::Number(n), .. } if n == expected)
        );
        let plan = compiler.compile_workbook(&workbook, &policy);
        let probe = EvaluationProbe::Formula {
            target: target(A1),
            formula: &formula,
        };
        let run = EvaluationRun::new_with_configuration(
            WorkbookView::new(&workbook, &plan, &policy).unwrap(),
            configuration,
            EvaluationBudgets::default(),
        )
        .unwrap()
        .with_dependency_sink(&mut reads);
        let _ = run.evaluate_probes(&[probe]);
        assert!(
            reads
                .0
                .iter()
                .any(|e| e.read == DependencyRead::NumberLocale)
        );
        assert!(!reads.0.iter().any(|e| matches!(
            e.read,
            DependencyRead::ContextField(ContextRead::AsOf | ContextRead::UtcOffset)
        )));
    }
}
