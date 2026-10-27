use super::support::{contract, identity, run, value, workbook};
use poirot::eval::{
    CapabilityId, EvaluationBudgets, EvaluationError, EvaluationInputs, EvaluationOutcome,
    ExtensionExecution, functions::extensions::*, functions::registry::Arity,
    run_formula_with_extensions,
};
use poirot::model::{CellAddress, CellError, CellValue};

#[test]
fn registered_callbacks_receive_contextual_operands_and_real_values() {
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "custom",
            "custom-v1",
            contract(Arity {
                minimum: 1,
                maximum: 1,
            }),
            identity,
        )
        .unwrap();
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(CellAddress { row: 0, col: 1 }, 42.)
        .unwrap();
    assert_eq!(
        value(run(&workbook, &registry, "cUsToM(B1)")),
        CellValue::Number(42.)
    );
    assert_eq!(
        value(run(&workbook, &registry, "SUM(CUSTOM({1,2;3,4}))")),
        CellValue::Number(10.)
    );
    assert_eq!(
        value(run(&workbook, &registry, "ISREF(CUSTOM(B1))")),
        CellValue::Boolean(true)
    );
    assert_eq!(
        value(run(&workbook, &registry, "CUSTOM(#DIV/0!)")),
        CellValue::Error(CellError::Div0)
    );
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "missing",
            "missing-v1",
            contract(Arity {
                minimum: 1,
                maximum: 1,
            }),
            |host: &mut dyn ExtensionHost| {
                Ok(ExtensionOutput::Scalar(CellValue::Boolean(
                    host.scalar(0)?.missing,
                )))
            },
        )
        .unwrap();
    assert_eq!(
        value(run(&workbook, &registry, "MISSING(,)")),
        CellValue::Error(CellError::Value)
    );
    // Distinguish an omitted argument from an actual blank worksheet cell.
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "ismissing",
            "missing-v2",
            contract(Arity {
                minimum: 2,
                maximum: 2,
            }),
            |host: &mut dyn ExtensionHost| {
                let first = host.scalar(0)?;
                let second = host.scalar(1)?;
                Ok(ExtensionOutput::Scalar(CellValue::Boolean(
                    first.missing && !second.missing,
                )))
            },
        )
        .unwrap();
    assert_eq!(
        value(run(&workbook, &registry, "ISMISSING(,C1)")),
        CellValue::Boolean(true)
    );
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "location",
            "location-v1",
            contract(Arity {
                minimum: 0,
                maximum: 0,
            }),
            |host: &mut dyn ExtensionHost| {
                let at = host.target()?;
                let _context = host.context()?;
                Ok(ExtensionOutput::Scalar(CellValue::Number(f64::from(
                    at.address.row + at.address.col + 1,
                ))))
            },
        )
        .unwrap();
    assert_eq!(
        value(run(&workbook, &registry, "LOCATION()")),
        CellValue::Number(1.)
    );
}

#[test]
fn reference_and_array_reads_are_budgeted_and_missing_is_preserved() {
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "last",
            "v1",
            contract(Arity {
                minimum: 1,
                maximum: 1,
            }),
            |host: &mut dyn ExtensionHost| {
                let shape = host.describe(0)?;
                let cell = host.read_cell(0, shape.rows - 1, shape.columns - 1)?;
                Ok(ExtensionOutput::Scalar(cell.value))
            },
        )
        .unwrap();
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(CellAddress { row: 1, col: 1 }, 33.)
        .unwrap();
    assert_eq!(
        value(run(&workbook, &registry, "LAST(A1:B2)")),
        CellValue::Number(33.)
    );
    assert_eq!(
        value(run(&workbook, &registry, "LAST({1,2;3,4})")),
        CellValue::Number(4.)
    );
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "costly",
            "v1",
            contract(Arity {
                minimum: 0,
                maximum: 0,
            }),
            |host: &mut dyn ExtensionHost| {
                host.charge_work(10_000_001)?;
                Ok(ExtensionOutput::Scalar(CellValue::Number(1.)))
            },
        )
        .unwrap();
    let unlimited = run(&workbook, &registry, "COSTLY()");
    assert!(unlimited.outcome.usage().work_units > 10_000_000);
    assert_eq!(value(unlimited), CellValue::Number(1.));
    for budgets in [
        EvaluationBudgets {
            run_work: 10_000_000,
            ..Default::default()
        },
        EvaluationBudgets {
            cell_work: 1_000_000,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            run_formula_with_extensions(
                &workbook,
                0,
                CellAddress { row: 0, col: 0 },
                "COSTLY()",
                ExtensionExecution {
                    registry: &registry,
                    context: EvaluationInputs::default(),
                    budgets,
                }
            )
            .outcome,
            EvaluationOutcome::Aborted {
                failure: EvaluationError::Budget(_),
                ..
            }
        ));
    }
}

#[test]
fn undeclared_capabilities_fail_operationally_and_arity_is_semantic() {
    let mut registry = ExtensionRegistry::new();
    let mut declared = contract(Arity {
        minimum: 1,
        maximum: 1,
    });
    declared.capabilities.references = false;
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "custom",
            "v1",
            declared,
            identity,
        )
        .unwrap();
    assert_eq!(
        value(run(&workbook(), &registry, "CUSTOM()")),
        CellValue::Error(CellError::Value)
    );
    assert!(matches!(
        run(&workbook(), &registry, "CUSTOM(A1)").outcome,
        EvaluationOutcome::Aborted {
            failure: EvaluationError::UnsupportedCapability {
                capability: CapabilityId::UserDefinedFunction,
                target: Some(_),
                ..
            },
            ..
        }
    ));
}
