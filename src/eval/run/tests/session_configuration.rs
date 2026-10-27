//! Public constructor contracts for borrowed execution settings.
use crate::eval::compile::compiler::{FormulaCompiler, PreparationPolicy};
use crate::eval::config::{
    ArrayResultPolicy, BuiltinSemanticsPolicy, CurrencyContext, EvaluationInputs,
    FormattingConfiguration, RunConfiguration,
};
use crate::eval::config::{EvaluationContext, EvaluationPolicy};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::functions::extensions::*;
use crate::eval::functions::registry::Arity;
use crate::eval::limits::{BudgetFailure, BudgetKind, EvaluationBudgets, EvaluationFailure};
use crate::eval::overlay::{Evaluation, Overlay};
use crate::eval::run::evaluation_run::EvaluationRun;
use crate::eval::run::workbook_view::WorkbookView;
use crate::model::{CellValue, Workbook};
use poirot_functions::text_units::{SourceVersion as TextSourceVersion, TextSemanticsPolicy};
use poirot_parsing::number::NumberLocale;

fn at(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}

fn workbook(formulas: &[(&str, &str)]) -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, formula) in formulas {
        sheet.set_formula(at(cell).address, *formula).unwrap();
    }
    book
}

fn registry() -> ExtensionRegistry {
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "CUSTOM",
            "identity-v1",
            ExtensionContract {
                arity: Arity {
                    minimum: 1,
                    maximum: 1,
                },
                evaluation: ExtensionEvaluationStrategy::Eager,
                capabilities: ExtensionCapabilities {
                    references: false,
                    arrays: false,
                    context: false,
                },
                cacheability: ExtensionCacheability::Deterministic,
            },
            |_host: &mut dyn ExtensionHost| -> ExtensionResult<ExtensionOutput> {
                Ok(ExtensionOutput::Argument(0))
            },
        )
        .unwrap();
    registry
}

fn formatting() -> FormattingConfiguration {
    FormattingConfiguration::with_currency(CurrencyContext {
        decimal_separator: ".".into(),
        grouping_separator: ",".into(),
        primary_group: 3,
        secondary_group: 3,
        positive_prefix: "USD ".into(),
        positive_suffix: String::new(),
        negative_prefix: "(USD ".into(),
        negative_suffix: ")".into(),
    })
}

#[test]
fn configured_extensions_retain_borrowed_settings_and_provider_binding() {
    // Existing execution_configuration and text_semantics061 contracts: explicit
    // currency, comma decimal parsing, and POI UTF-16 LEN are execution settings.
    let book = workbook(&[
        ("A1", "CUSTOM(DOLLAR(1))"),
        ("B1", "CUSTOM(\"1,234\"*1)"),
        ("C1", "CUSTOM(LEN(\"😀\"))"),
    ]);
    let registry = registry();
    let policy = PreparationPolicy::with_extensions(Default::default(), &registry);
    let plan = FormulaCompiler::new().compile_workbook_with_extensions(&book, &policy, &registry);
    let configuration = RunConfiguration::new(EvaluationInputs::default(), formatting())
        .with_number_locale(NumberLocale::new(",", Some("."), Some("EUR"), "%", ":").unwrap())
        .with_text_policy(TextSemanticsPolicy {
            source: TextSourceVersion::Poi7321209,
        });
    let mut run = EvaluationRun::new_with_configuration_and_extensions(
        WorkbookView::new(&book, &plan, &policy).unwrap(),
        &configuration,
        EvaluationBudgets::default(),
        &registry,
    )
    .unwrap();
    for (cell, expected) in [
        ("A1", CellValue::Text("USD 1.00".into())),
        ("B1", CellValue::Number(1.234)),
        ("C1", CellValue::Number(2.)),
    ] {
        assert_eq!(run.evaluate_cell(at(cell)).unwrap(), expected);
    }
}

#[test]
fn every_checked_constructor_validates_budgets_before_registry_identity() {
    let book = workbook(&[("A1", "CUSTOM(9)")]);
    let registry = registry();
    let wrong = ExtensionRegistry::new();
    let policy = PreparationPolicy::with_extensions(Default::default(), &registry);
    let plan = FormulaCompiler::new().compile_workbook_with_extensions(&book, &policy, &registry);
    let configuration = RunConfiguration::default();
    let view = || WorkbookView::new(&book, &plan, &policy).unwrap();
    for budgets in [
        EvaluationBudgets {
            depth: 129,
            ..Default::default()
        },
        EvaluationBudgets::default(),
    ] {
        for result in [
            EvaluationRun::new(view(), configuration.context(), budgets),
            EvaluationRun::new_with_extensions(view(), configuration.context(), budgets, &wrong),
            EvaluationRun::new_with_configuration(view(), &configuration, budgets),
            EvaluationRun::new_with_configuration_and_extensions(
                view(),
                &configuration,
                budgets,
                &wrong,
            ),
        ] {
            let failure = result.err().expect("invalid registry must fail");
            if budgets.depth == 129 {
                assert!(matches!(
                    failure,
                    EvaluationFailure::Budget(BudgetFailure {
                        kind: BudgetKind::Configuration,
                        requested: 129,
                        ..
                    })
                ));
            } else {
                assert_eq!(failure, EvaluationFailure::StalePlan);
            }
        }
    }
    let mut run = EvaluationRun::new_with_extensions(
        view(),
        configuration.context(),
        EvaluationBudgets::default(),
        &registry,
    )
    .unwrap();
    assert_eq!(run.evaluate_cell(at("A1")).unwrap(), CellValue::Number(9.));
}

#[test]
fn direct_and_overlay_combine_context_settings_with_workspace_policies() {
    // Reuse established contracts from execution_configuration, text_semantics061,
    // choose_array_selection, and dual_evaluation's scalar array publication tests.
    let book = workbook(&[
        ("A1", "DOLLAR(1)"),
        ("B1", "\"1,234\"*1"),
        ("C1", "LEN(\"😀\")"),
        ("D1", "SUM(CHOOSE({1,2,1},{3,100,7},5))"),
        ("E1", "{1,2;3,4}"),
    ]);
    let context = EvaluationContext {
        formatting: formatting(),
        number_locale: NumberLocale::new(",", Some("."), Some("EUR"), "%", ":").unwrap(),
        ..Default::default()
    };
    let policy = EvaluationPolicy {
        builtin: BuiltinSemanticsPolicy::PinnedPoi,
        arrays: ArrayResultPolicy::ScalarIntersection,
        text: TextSemanticsPolicy {
            source: TextSourceVersion::Poi7321209,
        },
        ..Default::default()
    };
    let mut direct = Evaluation::with_workbook(&book)
        .with_policy(policy.clone())
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy)
        .build()
        .unwrap();
    let mut evaluated = overlay.evaluation(&context).unwrap();
    for (cell, expected) in [
        ("A1", CellValue::Text("USD 1.00".into())),
        ("B1", CellValue::Number(1.234)),
        ("C1", CellValue::Number(2.)),
        // Array indices select each position: {3,5,7}, whose sum is 15.
        // https://support.microsoft.com/en-us/excel/functions/choose-function
        ("D1", CellValue::Number(15.)),
        ("E1", CellValue::Number(1.)),
        ("F2", CellValue::Blank),
    ] {
        assert_eq!(direct.cell(at(cell)).unwrap(), expected);
        assert_eq!(evaluated.cell(at(cell)).unwrap(), expected);
    }
}
