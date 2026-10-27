//! Prepared modal properties are source-bound and distinct from materialized cell values.
use crate::eval::compile::compiler::{FormulaCompiler, PreparationPolicy};
use crate::eval::config::ArrayResultPolicy;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::{compile, functions, overlay};
use crate::eval::{
    compile::compiler::PreparedFormula,
    functions::registry::{DependencyPolicy, ResultRule},
};
use crate::model::{CellValue, FormulaType, ScalarKinds, Workbook};

fn prepared(source: &str) -> std::sync::Arc<PreparedFormula> {
    FormulaCompiler::new().compile_formula(source, &PreparationPolicy::default())
}

fn properties(source: &str) -> compile::typing::ExpressionProperties {
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
    assert_eq!(
        root.result,
        FormulaType::scalar(ScalarKinds::NUMBER.join(ScalarKinds::ERROR))
    );
    assert!(!root.result.may_spill());
    assert!(
        root.effects
            .possible_reads
            .contains(DependencyPolicy::SEED.union(DependencyPolicy::TIME))
    );
    assert_eq!(
        root.effects.external,
        compile::typing::ExternalBehavior::Tracked
    );
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
            .contains(DependencyPolicy::TIME.union(DependencyPolicy::SEED))
    );
    assert_eq!(properties("unresolved_name").result, FormulaType::TOP);
}

#[test]
fn every_registry_entry_declares_an_alias_consistent_monotone_result_rule() {
    let arguments = [
        FormulaType::scalar(ScalarKinds::NUMBER),
        FormulaType::array(ScalarKinds::TEXT),
        FormulaType::TOP,
    ];
    {
        let descriptors = functions::registry::FUNCTION_DESCRIPTORS
            .iter()
            .collect::<Vec<_>>();
        for descriptor in &descriptors {
            for other in &descriptors {
                if descriptor.handler == other.handler {
                    assert_eq!(descriptor.result, other.result);
                }
            }
            for a in arguments {
                let b = a.join(FormulaType::array(ScalarKinds::NUMBER));
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
    assert!(!ResultRule::Number.apply(&[FormulaType::TOP]).may_spill());
}

#[test]
fn root_materialization_is_separate_from_expression_reference_form() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet.set_value("A1", 2.).unwrap();
    sheet.set_value("A2", 3.).unwrap();
    sheet.set_formula("D1", "TRANSPOSE(A1:A2)").unwrap();
    let target = EvaluationTarget {
        sheet: 0,
        address: "E1".parse().unwrap(),
    };
    for (arrays, expected) in [
        (ArrayResultPolicy::ScalarIntersection, CellValue::Blank),
        (ArrayResultPolicy::DynamicSpill, CellValue::Number(3.)),
    ] {
        let mut evaluation = overlay::Evaluation::with_workbook(&workbook)
            .with_policy(crate::eval::config::EvaluationPolicy {
                arrays,
                ..Default::default()
            })
            .build(crate::eval::config::EvaluationContext::default())
            .unwrap();
        assert_eq!(evaluation.cell(target).unwrap(), expected);
    }
}

#[test]
fn extension_result_capabilities_and_external_characteristics_compose() {
    use functions::extensions::*;
    for (cacheability, expected) in [
        (
            ExtensionCacheability::Deterministic,
            compile::typing::ExternalBehavior::Tracked,
        ),
        (
            ExtensionCacheability::Volatile,
            compile::typing::ExternalBehavior::Volatile,
        ),
        (
            ExtensionCacheability::Opaque,
            compile::typing::ExternalBehavior::Opaque,
        ),
    ] {
        let mut registry = ExtensionRegistry::new();
        registry
            .register(
                ExtensionNamespace::UserDefined,
                "MAKEARRAY",
                "types/v1",
                ExtensionContract {
                    arity: functions::registry::Arity {
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
        let policy = PreparationPolicy::with_extensions(Default::default(), &registry);
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
                    .contains(DependencyPolicy::TIME.union(DependencyPolicy::SEED))
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
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let first = compiler.compile_workbook(&workbook, &policy);
    let PreparedFormula::Ready(formula) = first.formula(target).unwrap().as_ref() else {
        panic!()
    };
    let original = formula.properties().unwrap();
    assert!(!original.result.may_spill());
    let snapshot = compile::compiler::PlanSnapshot::capture(&first)
        .unwrap()
        .into_owned();
    let bytes = postcard::to_allocvec(&snapshot).unwrap();
    let restored: compile::compiler::PlanSnapshot<'_> = postcard::from_bytes(&bytes).unwrap();
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
    let expected = FormulaType::multi_reference(ScalarKinds::ANY)
        .join(FormulaType::scalar(ScalarKinds::ERROR));
    assert_eq!(properties("A1#").result, expected);
    assert_eq!(properties("ANCHORARRAY(A1)").result, expected);
    assert_eq!(properties("_xlfn.ANCHORARRAY(A1)").result, expected);
    assert!(!properties("SUM(A1#)").result.may_spill());
    assert!(
        properties("A1#")
            .effects
            .possible_reads
            .contains(DependencyPolicy::REFERENCE_READS)
    );
    // These calls remain unresolved at policy-independent binding. The runtime
    // Excel profile can select a catalog handler, so typing must include its
    // result as well as the unresolved-name error for other profiles.
    for descriptor in functions::registry::FUNCTION_DESCRIPTORS.iter() {
        if functions::registry::resolve_function(&descriptor.name.into()).is_some() {
            continue;
        }
        let arguments = vec!["A1"; descriptor.arity.minimum];
        let source = format!("{}({})", descriptor.name, arguments.join(","));
        let input = FormulaType::single_reference(ScalarKinds::ANY)
            .join(FormulaType::scalar(ScalarKinds::ERROR));
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

#[test]
fn typed_operators_retain_broadcasting_and_conditional_argument_contracts() {
    use compile::typing::{ArgumentEvaluation, ConditionalArguments, ExpressionContract};
    for (source, wanted) in [
        ("IF(TRUE,1,RAND())", ConditionalArguments::If),
        ("IFERROR(1,NOW())", ConditionalArguments::IfError),
        ("IFNA(1,2)", ConditionalArguments::IfNa),
        ("CHOOSE(1,2,3)", ConditionalArguments::Choose),
        ("IFS(TRUE,1,FALSE,2)", ConditionalArguments::Ifs),
        ("SWITCH(1,1,2,3)", ConditionalArguments::Switch),
    ] {
        let formula = prepared(source);
        let PreparedFormula::Ready(formula) = formula.as_ref() else {
            panic!("valid conditional");
        };
        assert!(
            matches!(formula.bytecode().typing.expressions().first().unwrap().contract,
            ExpressionContract::Function {arguments:ArgumentEvaluation::Conditional(actual),..} if actual==wanted)
        );
    }
    for (source, kinds) in [
        ("{1;2;3}+{10,20}", ScalarKinds::NUMBER),
        ("{1;2;3}&{\"x\",\"y\"}", ScalarKinds::TEXT),
    ] {
        let formula = prepared(source);
        let PreparedFormula::Ready(formula) = formula.as_ref() else {
            panic!("valid broadcasting");
        };
        let root = formula.bytecode().typing.expressions().first().unwrap();
        assert_eq!(root.contract, ExpressionContract::Broadcast);
        assert_eq!(root.properties.shape, crate::model::ShapeBounds::new(3, 2));
        assert_eq!(root.properties.result.array, kinds.join(ScalarKinds::ERROR));
    }
    let formula = prepared("SUM({1,2;3,4})");
    let PreparedFormula::Ready(formula) = formula.as_ref() else {
        panic!("valid reduction");
    };
    let root = formula.bytecode().typing.expressions().first().unwrap();
    assert!(matches!(
        root.contract,
        ExpressionContract::Function {
            result: ResultRule::Number,
            ..
        }
    ));
    assert_eq!(root.properties.shape, crate::model::ShapeBounds::SCALAR);
    assert_eq!(
        root.properties.result,
        FormulaType::scalar(ScalarKinds::NUMBER.join(ScalarKinds::ERROR))
    );
}

#[test]
fn bounded_array_functions_preserve_maximum_dimensions_without_evaluating_arguments() {
    for source in [
        "SORT(A1:C10)",
        "SORTBY(A1:C10,B1:B10)",
        "UNIQUE(A1:C10)",
        "TAKE(A1:C10,3)",
        "DROP(A1:C10,3)",
        "MINVERSE(A1:C10)",
    ] {
        let root = properties(source);
        assert!(root.result.may_spill(), "{source}");
        assert_eq!(
            root.shape,
            crate::model::ShapeBounds::new(10, 3),
            "maximum bound for {source}"
        );
    }
    assert_eq!(
        properties("SORT(A:A)").shape,
        crate::model::ShapeBounds::new(crate::model::MAX_ROWS, 1)
    );
    assert_eq!(properties("SORT(SEQUENCE(A1,2))").shape.rows, None);
    assert_eq!(
        properties("SORT(SEQUENCE(A1,2))").shape.columns,
        std::num::NonZeroU32::new(2)
    );
}

#[test]
fn text_elementwise_contracts_prove_scalar_inputs_without_hiding_array_lifting() {
    for source in [
        "TEXTBEFORE(\"a,b\",\",\")",
        "TEXTAFTER(\"a,b\",\",\")",
        "VALUETOTEXT(12)",
    ] {
        assert_eq!(
            properties(source).result,
            FormulaType::scalar(ScalarKinds::TEXT.join(ScalarKinds::ERROR)),
            "{source}"
        );
    }
    for source in [
        "TEXTBEFORE({\"a,b\";\"c,d\"},\",\")",
        "TEXTAFTER({\"a,b\";\"c,d\"},\",\")",
        "VALUETOTEXT({1;2})",
    ] {
        assert_eq!(
            properties(source).result.array,
            ScalarKinds::TEXT.join(ScalarKinds::ERROR),
            "{source}"
        );
        assert_eq!(
            properties(source).shape,
            crate::model::ShapeBounds::new(2, 1),
            "{source}"
        );
    }
}

#[test]
fn text_extraction_fallbacks_keep_numeric_boolean_and_missing_kinds() {
    for (source, kind) in [
        ("TEXTBEFORE(\"a\",\",\",1,0,0,42)", ScalarKinds::NUMBER),
        ("TEXTAFTER(\"a\",\",\",1,0,0,TRUE)", ScalarKinds::BOOLEAN),
        ("TEXTBEFORE(\"a\",\",\",1,0,0,)", ScalarKinds::NUMBER),
    ] {
        assert!(
            kind.is_subtype_of(properties(source).result.scalar),
            "{source}"
        );
        assert!(!properties(source).result.may_spill(), "{source}");
    }
}

#[test]
fn array_transforms_propagate_data_kinds_without_confusing_control_arguments_with_output() {
    for (source, kinds) in [
        ("SEQUENCE(3,2)", ScalarKinds::NUMBER),
        ("SORT({3;1;2})", ScalarKinds::NUMBER),
        ("SORT({\"b\";\"a\"})", ScalarKinds::TEXT),
        ("UNIQUE({TRUE;FALSE;TRUE})", ScalarKinds::BOOLEAN),
        ("TAKE({1;2},1)", ScalarKinds::NUMBER),
        (
            "HSTACK({1;2},{\"a\";\"b\"})",
            ScalarKinds::NUMBER.join(ScalarKinds::TEXT),
        ),
        ("TRANSPOSE({\"a\",\"b\"})", ScalarKinds::TEXT),
        ("FILTER({1;2},{TRUE;FALSE})", ScalarKinds::NUMBER),
        (
            "FILTER({1;2},{FALSE;FALSE},\"empty\")",
            ScalarKinds::NUMBER.join(ScalarKinds::TEXT),
        ),
        ("MMULT({1,2},{3;4})", ScalarKinds::NUMBER),
        ("MINVERSE({1,0;0,1})", ScalarKinds::NUMBER),
    ] {
        assert_eq!(
            properties(source).result.array,
            kinds.join(ScalarKinds::ERROR),
            "{source}"
        );
    }
}
