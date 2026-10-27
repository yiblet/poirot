use super::super::compiled_formula::CompiledFormula;
use super::super::formula_compiler::FormulaCompiler;
use super::super::potential_dependencies::potential;
use super::super::preparation_policy::PreparationPolicy;
use super::super::prepared_formula::PreparedFormula;
use super::super::templates::executable_hash;
use crate::eval::compile::bound;
use poirot_parsing::formula::{FormulaParsePolicy, parse_formula_with_policy};
use std::sync::Arc;

fn ready(formula: &PreparedFormula) -> &CompiledFormula {
    match formula {
        PreparedFormula::Ready(formula) => formula,
        _ => panic!("expected ready"),
    }
}

#[test]
fn classified_hit_shares_complete_program_without_parsing_or_binding() {
    let mut compiler = FormulaCompiler::new();
    let policy = PreparationPolicy::default();
    let first = compiler.compile_formula("VLOOKUP(A1,'O''Brien'!$B$2:C3,2,FALSE)", &policy);
    let second = compiler.compile_formula("VLOOKUP(Z9,'O''Brien'!D4:$E$5,2,FALSE)", &policy);
    let (a, b) = (ready(&first), ready(&second));
    assert_eq!(compiler.usage().parse_attempts, 1);
    assert_eq!(compiler.usage().bind_attempts, 1);
    assert_eq!(compiler.template_hits(), 1);
    assert_eq!(compiler.unique_programs(), 1);
    assert!(Arc::ptr_eq(
        &a.bytecode().executable,
        &b.bytecode().executable
    ));
    assert_ne!(a.bytecode().references, b.bytecode().references);
    let parsed = parse_formula_with_policy(second.source(), policy.parse).unwrap();
    let expected_program = crate::eval::compile::bytecode::compile(parsed.expression()).unwrap();
    assert_eq!(
        b.potential,
        potential(parsed.expression(), &expected_program).unwrap()
    );
    assert_eq!(
        b.bytecode().references,
        crate::eval::compile::bytecode::compile(parsed.expression())
            .unwrap()
            .references
    );
    assert_eq!(b.parsed().unwrap().source(), second.source());
    assert_ne!(b.parsed().unwrap().source(), a.parsed().unwrap().source());
}

#[test]
fn structural_interning_covers_unsupported_and_differently_spelled_sources() {
    let mut compiler = FormulaCompiler::new();
    let policy = PreparationPolicy::default();
    for source in ["SUM('One'!A:A)", "SUM('Two'!B:B)", "SUM ( 'Three'!$C:$C )"] {
        compiler.compile_formula(source, &policy);
    }
    assert_eq!(compiler.usage().parse_attempts, 3);
    assert_eq!(compiler.unique_programs(), 1);
    assert_eq!(compiler.template_hits(), 0);
}

#[test]
fn normalized_hits_preserve_literals_call_roles_and_policy() {
    let mut compiler = FormulaCompiler::new();
    let policy = PreparationPolicy::default();
    for source in [
        "SUM(A1,1)",
        "SUM(B2,2)",
        "SUM(C3,Name)",
        "SUM(D4,Other)",
        "SUM(E5,1)+1",
        "sum(F6,1)",
        "LOG10(A1)",
        "A1(A2)",
        "SUM(A1,\"B2\")",
    ] {
        compiler.compile_formula(source, &policy);
    }
    assert_eq!(compiler.template_hits(), 0);
    let mut limited = policy.clone();
    limited.parse.max_rows = 2;
    compiler.compile_formula("SUM(A1,1)", &limited);
    assert!(matches!(
        compiler.compile_formula("SUM(A3,1)", &limited).as_ref(),
        PreparedFormula::ParseFailure { .. }
    ));
    assert_eq!(compiler.template_hits(), 0);
}

#[test]
fn forced_normalized_bucket_collision_requires_complete_equality() {
    let mut compiler = FormulaCompiler::new();
    let policy = PreparationPolicy::default();
    let first = compiler.compile_formula("SUM(A1,1)", &policy);
    let hash = poirot_parsing::formula::classified::normalized_hash("SUM(B2,2)").unwrap();
    let PreparedFormula::Ready(first) = first.as_ref() else {
        panic!()
    };
    compiler.templates[0]
        .classified
        .insert(hash, vec![Arc::clone(first)]);
    compiler.compile_formula("SUM(B2,2)", &policy);
    assert_eq!(compiler.usage().parse_attempts, 2);
    assert_eq!(compiler.template_hits(), 0);
}

#[test]
fn extension_hit_reconstructs_exact_stored_namespace_without_provider_capture() {
    use crate::eval::functions::extensions::*;
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "CUSTOM",
            "v1",
            ExtensionContract {
                arity: crate::eval::functions::registry::Arity {
                    minimum: 1,
                    maximum: 1,
                },
                evaluation: ExtensionEvaluationStrategy::Deferred,
                capabilities: ExtensionCapabilities {
                    references: true,
                    arrays: false,
                    context: false,
                },
                cacheability: ExtensionCacheability::Deterministic,
            },
            |_: &mut dyn ExtensionHost| -> ExtensionResult<ExtensionOutput> {
                panic!("preparation must not execute providers")
            },
        )
        .unwrap();
    registry
        .set_external_namespace(Some(
            crate::eval::functions::external_namespace::ExternalFunctionNamespace::from_stored_names(
                ["CuStOm", "sUm", "mYsTeRy"],
                4,
            )
            .unwrap(),
        ))
        .unwrap();
    let policy = PreparationPolicy::with_extensions(FormulaParsePolicy::default(), &registry);
    let mut compiler = FormulaCompiler::new();
    compiler.compile_formula_with_extensions("SUM(CUSTOM(A1),MYSTERY(B2))", &policy, &registry);
    let second =
        compiler.compile_formula_with_extensions("SUM(CUSTOM(C3),MYSTERY(D4))", &policy, &registry);
    let formula = ready(&second);
    assert_eq!(compiler.usage().parse_attempts, 1);
    let expected =
        bound::bind_with_registry(formula.parsed().unwrap().expression(), Some(&registry));
    drop(registry);
    drop(compiler);
    assert_eq!(formula.executable(), expected);
}

#[test]
fn forced_structural_bucket_collision_requires_complete_equality() {
    let mut compiler = FormulaCompiler::new();
    let policy = PreparationPolicy::default();
    let first = compiler.compile_formula("1+2", &policy);
    let other = crate::eval::compile::bytecode::compile(
        &poirot_parsing::formula::parse_formula("1+3").unwrap(),
    )
    .unwrap();
    let hash = executable_hash(&other.executable);
    compiler.templates[0]
        .structural
        .insert(hash, vec![Arc::clone(&ready(&first).bytecode().executable)]);
    let second = compiler.compile_formula("1+3", &policy);
    assert!(!Arc::ptr_eq(
        &ready(&first).bytecode().executable,
        &ready(&second).bytecode().executable
    ));
    assert_eq!(compiler.templates[0].structural[&hash].len(), 2);
}

#[test]
fn registry_scopes_templates_and_lazy_call_binding() {
    let mut compiler = FormulaCompiler::new();
    let registry = crate::eval::functions::extensions::ExtensionRegistry::new();
    let policy = PreparationPolicy::with_extensions(FormulaParsePolicy::default(), &registry);
    let first =
        compiler.compile_formula_with_extensions("IFERROR(A1,UNKNOWN(B2))", &policy, &registry);
    let second =
        compiler.compile_formula_with_extensions("IFERROR(C3,UNKNOWN(D4))", &policy, &registry);
    let b = ready(&second);
    assert_eq!(compiler.template_hits(), 1);
    assert_eq!(
        b.executable(),
        bound::bind_with_registry(b.parsed().unwrap().expression(), Some(&registry))
    );
    let third = compiler.compile_formula("IFERROR(E5,UNKNOWN(F6))", &PreparationPolicy::default());
    assert!(!Arc::ptr_eq(
        &ready(&first).bytecode().executable,
        &ready(&third).bytecode().executable
    ));
}

#[test]
fn operator_typing_preserves_point_templates_and_recompiles_range_bindings() {
    let mut compiler = FormulaCompiler::new();
    let policy = PreparationPolicy::default();
    let first = compiler.compile_formula("A1+IF(TRUE,B2,3)", &policy);
    let second = compiler.compile_formula("C3+IF(TRUE,D4,3)", &policy);
    assert_eq!(compiler.template_hits(), 1);
    assert!(!ready(&first).properties().unwrap().result.may_spill());
    assert!(!ready(&second).properties().unwrap().result.may_spill());
    let short = compiler.compile_formula("A1:A2+1", &policy);
    let long = compiler.compile_formula("B1:B4+1", &policy);
    assert_eq!(compiler.template_hits(), 1);
    assert!(ready(&short).properties().unwrap().result.may_spill());
    assert!(ready(&long).properties().unwrap().result.may_spill());
}
