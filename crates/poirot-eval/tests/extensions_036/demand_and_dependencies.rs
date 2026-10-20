use super::support::{contract, identity, run, value, workbook};
use poirot_eval::{EvaluationBudgets, EvaluationInputs, extensions::*, registry::Arity};
use poirot_model::{CellAddress, CellError, CellValue};

#[test]
fn opaque_and_registered_skipped_calls_have_no_consumed_trace() {
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "bad",
            "bad-v1",
            contract(Arity {
                minimum: 0,
                maximum: 0,
            }),
            |_: &mut dyn ExtensionHost| -> ExtensionResult<ExtensionOutput> {
                panic!("skipped callback executed")
            },
        )
        .unwrap();
    let workbook = workbook();
    for formula in ["IF(FALSE,BAD(),7)", "IF(TRUE,7,UNKNOWN())"] {
        let result = run(&workbook, &registry, formula);
        assert!(result.trace.invocations.is_empty());
        assert_eq!(value(result), CellValue::Number(7.));
    }
    assert_eq!(
        value(run(&workbook, &registry, "UNKNOWN()")),
        CellValue::Error(CellError::Name)
    );
}

#[test]
fn cell_dependencies_use_the_same_registry_and_record_their_source_cell() {
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "custom",
            "v1",
            contract(Arity {
                minimum: 1,
                maximum: 1,
            }),
            identity,
        )
        .unwrap();
    let mut workbook = workbook();
    let dependency = CellAddress { row: 0, col: 1 };
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value(dependency, "CUSTOM(12)", CellValue::Number(-1.))
        .unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value(
            CellAddress { row: 0, col: 0 },
            "B1+1",
            CellValue::Number(-2.),
        )
        .unwrap();
    let result = run_cell_with_extensions(
        &workbook,
        0,
        CellAddress { row: 0, col: 0 },
        ExtensionExecution {
            registry: &registry,
            context: EvaluationInputs::default(),
            budgets: EvaluationBudgets::default(),
        },
    );
    assert_eq!(result.trace.invocations.len(), 1);
    assert_eq!(result.trace.invocations[0].target.address, dependency);
    assert_eq!(value(result), CellValue::Number(13.));
}

#[test]
fn explicit_deferred_provider_can_skip_an_opaque_argument() {
    let mut registry = ExtensionRegistry::new();
    let mut declared = contract(Arity {
        minimum: 1,
        maximum: 1,
    });
    declared.evaluation = ExtensionEvaluationStrategy::Deferred;
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "skip",
            "v1",
            declared,
            |_host: &mut dyn ExtensionHost| Ok(ExtensionOutput::Scalar(CellValue::Number(19.))),
        )
        .unwrap();
    assert_eq!(
        value(run(&workbook(), &registry, "SKIP(UNKNOWN())")),
        CellValue::Number(19.)
    );
}
