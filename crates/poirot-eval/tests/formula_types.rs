//! Prepared modal properties are source-bound and distinct from materialized cell values.
use poirot_eval::{
    compiler::PreparedFormula,
    registry::{DependencyPolicy as D, ResultRule},
    *,
};
use poirot_model::{CellValue, FormulaType as T, ScalarKinds as K, Workbook};

fn prepared(source: &str) -> std::sync::Arc<PreparedFormula> {
    FormulaCompiler::new().compile_formula(source, &EvaluationPolicy::default())
}
fn properties(source: &str) -> typing::ExpressionProperties {
    let p = prepared(source);
    let PreparedFormula::Ready(formula) = p.as_ref() else {
        panic!()
    };
    formula.properties().unwrap()
}
#[test]
fn scalar_consumers_discard_array_forms_but_keep_modal_characteristics() {
    let p = prepared("SUM(TRANSPOSE(CHOOSE({1,2},RAND(),NOW())))");
    let PreparedFormula::Ready(formula) = p.as_ref() else {
        panic!()
    };
    let root = formula.properties().unwrap();
    assert_eq!(root.result, T::scalar(K::NUMBER.join(K::ERROR)));
    assert!(!root.result.may_spill());
    assert!(root.effects.possible_reads.contains(D::SEED.union(D::TIME)));
    assert_eq!(root.effects.external, typing::ExternalBehavior::Tracked);
    assert!(
        formula
            .bytecode()
            .typing
            .expressions()
            .iter()
            .any(|e| e.properties.result.may_spill())
    );
    assert!(!properties("A1").result.may_spill());
    assert!(properties("A1:B2").result.may_spill());
    assert!(!properties("@A1:B2").result.may_spill());
    assert!(!properties("VLOOKUP(1,A1:B2,2,FALSE)").result.may_spill());
    assert!(
        !properties("IFERROR(VLOOKUP(1,A1:B2,2,FALSE),0)")
            .result
            .may_spill()
    );
}
#[test]
fn selection_types_include_array_conditions_and_indices() {
    for source in [
        "IF({TRUE,FALSE},1,2)",
        "IFS({TRUE,FALSE},1,TRUE,2)",
        "CHOOSE({1,2},1,2)",
        "SWITCH({1,2},1,3,2,4)",
    ] {
        assert!(properties(source).result.may_spill(), "{source}");
    }
    assert!(
        properties("IF(FALSE,NOW(),RAND())")
            .effects
            .possible_reads
            .contains(D::TIME.union(D::SEED))
    );
    assert_eq!(properties("unresolved_name").result, T::TOP);
}
#[test]
fn every_registry_entry_declares_an_alias_consistent_monotone_result_rule() {
    let arguments = [T::scalar(K::NUMBER), T::array(K::TEXT), T::TOP];
    for policy in [
        BuiltinSemanticsPolicy::PinnedPoi,
        BuiltinSemanticsPolicy::ExcelDocumentation,
    ] {
        let descriptors = registry::descriptors_for_policy(policy).collect::<Vec<_>>();
        for descriptor in &descriptors {
            for other in &descriptors {
                if descriptor.handler == other.handler {
                    assert_eq!(descriptor.result, other.result);
                }
            }
            for a in arguments {
                let b = a.join(T::array(K::NUMBER));
                assert!(
                    descriptor
                        .result
                        .apply(&[a, a, a])
                        .is_subtype_of(descriptor.result.apply(&[b, b, b])),
                    "{}",
                    descriptor.name
                );
            }
        }
    }
    assert!(!ResultRule::Number.apply(&[T::TOP]).may_spill());
}
#[test]
fn root_materialization_is_separate_from_expression_reference_form() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet.set_value("A1".parse().unwrap(), 2.).unwrap();
    sheet.set_value("A2".parse().unwrap(), 3.).unwrap();
    sheet
        .set_formula("D1".parse().unwrap(), "TRANSPOSE(A1:A2)")
        .unwrap();
    let target = EvaluationTarget {
        sheet: 0,
        address: "E1".parse().unwrap(),
    };
    for (arrays, expected) in [
        (ArrayResultPolicy::ScalarIntersection, CellValue::Blank),
        (ArrayResultPolicy::DynamicSpill, CellValue::Number(3.)),
    ] {
        let mut evaluation = overlay::Evaluation::with_workbook(&workbook)
            .with_policy(overlay::OverlayPolicy {
                arrays,
                ..Default::default()
            })
            .build(overlay::EvaluationContext::default())
            .unwrap();
        assert_eq!(evaluation.cell(target).unwrap(), expected);
    }
}

#[test]
fn extension_result_capabilities_and_external_characteristics_compose() {
    use extensions::*;
    for (cacheability, expected) in [
        (
            ExtensionCacheability::Deterministic,
            typing::ExternalBehavior::Tracked,
        ),
        (
            ExtensionCacheability::Volatile,
            typing::ExternalBehavior::Volatile,
        ),
        (
            ExtensionCacheability::Opaque,
            typing::ExternalBehavior::Opaque,
        ),
    ] {
        let mut registry = ExtensionRegistry::new();
        registry
            .register(
                ExtensionNamespace::UserDefined,
                "MAKEARRAY",
                "types/v1",
                ExtensionContract {
                    arity: registry::Arity {
                        minimum: 0,
                        maximum: 0,
                    },
                    evaluation: ExtensionEvaluationStrategy::Eager,
                    capabilities: ExtensionCapabilities {
                        references: false,
                        arrays: true,
                        context: true,
                    },
                    cacheability,
                },
                |_: &mut dyn ExtensionHost| {
                    Ok(ExtensionOutput::Array(vec![vec![
                        CellValue::Number(1.),
                        CellValue::Number(2.),
                    ]]))
                },
            )
            .unwrap();
        let mut compiler = FormulaCompiler::new();
        let policy = EvaluationPolicy::with_extensions(Default::default(), &registry);
        for (source, spills) in [("MAKEARRAY()", true), ("SUM(MAKEARRAY())", false)] {
            let p = compiler.compile_formula_with_extensions(source, &policy, &registry);
            let PreparedFormula::Ready(formula) = p.as_ref() else {
                panic!()
            };
            let properties = formula.properties().unwrap();
            assert_eq!(properties.result.may_spill(), spills);
            assert_eq!(properties.effects.external, expected);
            assert!(
                properties
                    .effects
                    .possible_reads
                    .contains(D::TIME.union(D::SEED))
            );
        }
    }
}

#[test]
fn prepared_root_properties_follow_exact_source_and_survive_snapshot_restore() {
    let mut workbook = Workbook::new();
    let address = "A1".parse().unwrap();
    workbook
        .add_sheet("S")
        .unwrap()
        .set_formula(address, "SUM({1,2})")
        .unwrap();
    let target = EvaluationTarget { sheet: 0, address };
    let policy = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let first = compiler.compile_workbook(&workbook, &policy);
    let PreparedFormula::Ready(formula) = first.formula(target).unwrap().as_ref() else {
        panic!()
    };
    let original = formula.properties().unwrap();
    assert!(!original.result.may_spill());
    let snapshot = compiler::snapshot::Plan::capture(&first)
        .unwrap()
        .into_owned();
    let bytes = postcard::to_allocvec(&snapshot).unwrap();
    let restored: compiler::snapshot::Plan<'_> = postcard::from_bytes(&bytes).unwrap();
    let restored = restored.restore().unwrap();
    let PreparedFormula::Ready(formula) = restored.formula(target).unwrap().as_ref() else {
        panic!()
    };
    assert_eq!(formula.properties().unwrap(), original);
    assert!(restored.matches(&workbook, &policy));
    // Direct mutation of the public source field cannot leave an authoritative stale flag.
    workbook
        .sheet_mut(0)
        .unwrap()
        .get_mut(address)
        .unwrap()
        .formula = Some("{1,2}".into());
    assert!(!first.matches(&workbook, &policy));
    let changed = compiler.compile_workbook(&workbook, &policy);
    let PreparedFormula::Ready(formula) = changed.formula(target).unwrap().as_ref() else {
        panic!()
    };
    assert!(formula.properties().unwrap().result.may_spill());
}

#[test]
fn spill_reference_and_runtime_profile_fallback_use_reference_result_contracts() {
    let expected = T::multi_reference(K::ANY).join(T::scalar(K::ERROR));
    assert_eq!(properties("A1#").result, expected);
    assert_eq!(properties("ANCHORARRAY(A1)").result, expected);
    assert_eq!(properties("_xlfn.ANCHORARRAY(A1)").result, expected);
    assert!(!properties("SUM(A1#)").result.may_spill());
    assert!(
        properties("A1#")
            .effects
            .possible_reads
            .contains(D::REFERENCE_READS)
    );
    // These calls remain unresolved at policy-independent binding. The runtime
    // Excel profile can select a catalog handler, so typing must include its
    // result as well as the unresolved-name error for other profiles.
    for descriptor in registry::descriptors_for_policy(BuiltinSemanticsPolicy::ExcelDocumentation) {
        if registry::resolve_function(&descriptor.name.into()).is_some() {
            continue;
        }
        let arguments = vec!["A1"; descriptor.arity.minimum];
        let source = format!("{}({})", descriptor.name, arguments.join(","));
        let input = T::single_reference(K::ANY).join(T::scalar(K::ERROR));
        let result = descriptor
            .result
            .apply(&vec![input; descriptor.arity.minimum]);
        let actual = properties(&source);
        assert!(result.is_subtype_of(actual.result), "{source}");
        assert!(
            actual
                .effects
                .possible_reads
                .contains(descriptor.dependencies)
        );
    }
}
