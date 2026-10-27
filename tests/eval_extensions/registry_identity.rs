use super::support::{contract, identity, run, value, workbook};
use poirot::eval::{
    CapabilityId, EvaluationBudgets, EvaluationFailure, EvaluationInputs, EvaluationOutcome,
    ExtensionExecution,
    compile::bound::{Expression, FormulaPlan},
    functions::extensions::*,
    functions::registry::Arity,
    run_plan_with_extensions,
};
use poirot::model::{CellAddress, CellValue};

#[test]
fn trace_contract_and_epoch_make_provider_changes_visible() {
    let mut registry = ExtensionRegistry::new();
    let mut declared = contract(Arity {
        minimum: 1,
        maximum: 1,
    });
    declared.cacheability = ExtensionCacheability::Volatile;
    let binding = registry
        .register(
            ExtensionNamespace::UserDefined,
            "custom",
            "revision-a",
            declared,
            identity,
        )
        .unwrap();
    let syntax = poirot_parsing::formula::parse_formula("CUSTOM(5)").unwrap();
    let plan = FormulaPlan::bind_with_extensions(&syntax, &registry).unwrap();
    assert!(matches!(plan.expression(), Expression::Extension { .. }));
    let workbook = workbook();
    let result = run(&workbook, &registry, "CUSTOM(5)");
    assert_eq!(result.trace.invocations[0].binding, binding);
    let before = registry.identity();
    registry
        .replace(
            ExtensionNamespace::UserDefined,
            "CUSTOM",
            "revision-b",
            declared,
            identity,
        )
        .unwrap();
    assert_ne!(before, registry.identity());
    let stale = run_plan_with_extensions(
        &workbook,
        0,
        CellAddress { row: 0, col: 0 },
        &plan,
        ExtensionExecution {
            registry: &registry,
            context: EvaluationInputs::default(),
            budgets: EvaluationBudgets::default(),
        },
    );
    assert!(matches!(
        stale.outcome,
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::UnsupportedCapability {
                capability: CapabilityId::UserDefinedFunction,
                ..
            },
            ..
        }
    ));
}

#[test]
fn structural_identity_includes_bound_ids_for_independently_constructed_registries() {
    let declared = contract(Arity {
        minimum: 1,
        maximum: 1,
    });
    let mut first = ExtensionRegistry::new();
    let mut reversed = ExtensionRegistry::new();
    let mut equivalent = ExtensionRegistry::new();
    for (registry, order) in [
        (&mut first, ["POIROT_TEST_FIRST", "POIROT_TEST_SECOND"]),
        (&mut reversed, ["POIROT_TEST_SECOND", "POIROT_TEST_FIRST"]),
        (&mut equivalent, ["POIROT_TEST_FIRST", "POIROT_TEST_SECOND"]),
    ] {
        for name in order {
            registry
                .register(
                    ExtensionNamespace::UserDefined,
                    name,
                    "same-signature",
                    declared,
                    identity,
                )
                .unwrap();
        }
    }
    let first_binding = first.resolve("poirot_TEST_FIRST").unwrap();
    let reversed_binding = reversed.resolve("poirot_TEST_FIRST").unwrap();
    assert_ne!(first_binding.id, reversed_binding.id);
    assert_eq!(first_binding.id, first_binding.registration.binding_id);
    assert_eq!(
        reversed_binding.id,
        reversed_binding.registration.binding_id
    );
    assert_ne!(first.identity(), reversed.identity());
    assert_eq!(first.identity(), equivalent.identity());
    let syntax = poirot_parsing::formula::parse_formula("poirot_TEST_FIRST(7)").unwrap();
    let plan = FormulaPlan::bind_with_extensions(&syntax, &first).unwrap();
    let workbook = workbook();
    let run_plan = |registry| {
        run_plan_with_extensions(
            &workbook,
            0,
            CellAddress { row: 0, col: 0 },
            &plan,
            ExtensionExecution {
                registry,
                context: EvaluationInputs::default(),
                budgets: EvaluationBudgets::default(),
            },
        )
    };
    assert_eq!(value(run_plan(&equivalent)), CellValue::Number(7.));
    assert!(matches!(
        run_plan(&reversed).outcome,
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::UnsupportedCapability {
                capability: CapabilityId::UserDefinedFunction,
                ..
            },
            ..
        }
    ));
}
