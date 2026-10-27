use super::super::cache_snapshot;
use super::super::formula_compiler::FormulaCompiler;
use super::super::preparation_policy::PreparationPolicy;
use crate::model::ShapeBounds;
use poirot_parsing::formula::FormulaParsePolicy;

fn check(compiler: &mut FormulaCompiler, source: &str, policy: &PreparationPolicy) {
    // A separate compiler performs authoritative full binding and inference.
    let expected = FormulaCompiler::new()
        .compile_formula(source, policy)
        .spill_shape();
    assert_eq!(
        compiler.spill_shape(source, policy, None),
        expected,
        "{source}"
    );
}

#[test]
fn scalar_proofs_do_not_materialize_coordinate_or_literal_variants() {
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    for row in 1..=128 {
        let source = format!("IFERROR(VLOOKUP(A{row},'O''Brien'!$B:$C,2,FALSE),\"missing {row}\")");
        check(&mut compiler, &source, &policy);
    }
    assert_eq!(compiler.usage().parse_attempts, 1);
    assert_eq!(compiler.pool[0].1.len(), 1);
    assert_eq!(compiler.templates[0].scalar.len(), 1);
    // Classification cannot substitute a stale coordinate binding when reached.
    let actual = compiler.compile_formula(
        "IFERROR(VLOOKUP(A128,'O''Brien'!$B:$C,2,FALSE),\"missing 128\")",
        &policy,
    );
    assert_eq!(
        actual.source(),
        "IFERROR(VLOOKUP(A128,'O''Brien'!$B:$C,2,FALSE),\"missing 128\")"
    );
    assert_eq!(compiler.pool[0].1.len(), 2);
}

#[test]
fn scalar_proofs_agree_with_full_inference_across_result_forms() {
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    for row in 1..=8 {
        for number in [1, 3, 17] {
            for source in [
                format!("SUM(A{row}:C{number})"),
                format!("SUM('S'!{row}:{number})"),
                format!("SUM(SEQUENCE({number}))"),
                format!("SUM(TRANSPOSE(SEQUENCE({number},2)))"),
                format!("@SEQUENCE({number})"),
                format!("IF(TRUE,A{row},{number})"),
                format!("IFERROR(A{row},\"a\"\"{number}\")"),
                format!("IF(TRUE,SEQUENCE({number}),A{row})"),
                format!("IFERROR(SEQUENCE({number}),A{row})"),
                format!("INDEX(A{row}:C{number},{number})"),
                format!("CHOOSE({number},A{row},SEQUENCE({number}))"),
                format!("SUM(INDEX(A{row}:C{number},{number}))"),
                format!("ISNUMBER(SEQUENCE({number}))"),
                format!("unknown(A{row},{number})"),
                format!("SUM(unresolved_name,A{row})"),
                format!("A{row}+{number}"),
            ] {
                check(&mut compiler, &source, &policy);
            }
        }
    }
    for source in [
        "{1;2}",
        "SUM({1;2})",
        "#N/A",
        "IFERROR(#N/A,1)",
        "SUM(S!name)",
        "SEQUENCE(RAND())",
        "A1#",
        "SUM(A1#)",
        "TRUE",
        "FALSE",
        "LOG10(A1)",
        "log10(A2)",
    ] {
        check(&mut compiler, source, &policy);
    }
}

#[test]
fn array_geometry_is_never_reused_from_scalar_proof_keys() {
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    for (source, expected) in [
        ("SEQUENCE(3,2)", ShapeBounds::new(3, 2)),
        ("SEQUENCE(8,4)", ShapeBounds::new(8, 4)),
        ("A1:B3", ShapeBounds::new(3, 2)),
        ("C2:H9", ShapeBounds::new(8, 6)),
        ("TRANSPOSE(C2:H9)", ShapeBounds::new(6, 8)),
    ] {
        assert_eq!(compiler.spill_shape(source, &policy, None), Some(expected));
    }
    assert!(compiler.templates[0].scalar.is_empty());
}

#[test]
fn grammar_grid_and_policy_failures_cannot_acquire_a_scalar_proof() {
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    check(&mut compiler, "SUM(A1,1,\"ok\")", &policy);
    for source in [
        "SUM(A1048577,1,\"ok\")",
        "SUM(XFE1,1,\"ok\")",
        "SUM(A2,1e999,\"ok\")",
        "SUM(A2,1,\"unterminated)",
        "SUM(A2,1,,)",
        "SUM(A2,1,\"ok\"))",
        "SUM(A:A1)",
        "SUM('S'!A:1)",
        "SUM(A0:A1)",
        "SUM('S'!0:1)",
    ] {
        check(&mut compiler, source, &policy);
    }
    for limited in [
        FormulaParsePolicy {
            max_expressions: 1,
            ..policy.parse
        },
        FormulaParsePolicy {
            max_depth: 1,
            ..policy.parse
        },
        FormulaParsePolicy {
            max_rows: 4,
            max_columns: 4,
            ..policy.parse
        },
        FormulaParsePolicy {
            max_rows: 0,
            ..policy.parse
        },
    ] {
        let policy = PreparationPolicy {
            parse: limited,
            ..policy.clone()
        };
        for source in [
            "SUM(A1,1)",
            "SUM(A2,2)",
            "SUM(A5,2)",
            "SUM('S'!A:E)",
            "SUM('S'!1:5)",
        ] {
            check(&mut compiler, source, &policy);
        }
    }
}

#[test]
fn literal_kinds_and_qualifiers_preserve_syntax_distinctions() {
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    for source in [
        "SUM(123!A1,1)",
        "SUM(456!B2,2)",
        "SUM('123'!A1,1)",
        "SUM('456'!B2,2)",
        "IFERROR(A1,\"a\"\"b\")",
        "IFERROR(B2,\"longer text\")",
        "SUM(A1,1.25e-3)",
        "SUM(B2,.5)",
        "SUM(C3,2E+4)",
        "SUM('O''Brien'!$A:$C)",
        "SUM('O''Brien'!$1:$3)",
        "SUM((A1:B2,C3:D4))",
        "SUM(A1:B2 C2:D3)",
    ] {
        check(&mut compiler, source, &policy);
    }
}

#[test]
fn every_catalog_transfer_keeps_literal_variants_conservative() {
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    for descriptor in crate::eval::functions::registry::FUNCTION_DESCRIPTORS.iter() {
        for (cell, number, text) in [("A1", 1, "a"), ("B2", 7, "longer")] {
            for source in [
                format!("{}({cell},{number},\"{text}\")", descriptor.name),
                format!(
                    "SUM({}(SEQUENCE({number}),{cell},\"{text}\"))",
                    descriptor.name
                ),
            ] {
                check(&mut compiler, &source, &policy);
            }
        }
    }
}

#[test]
fn extension_argument_forms_and_namespaces_remain_conservative() {
    use crate::eval::functions::extensions::*;
    for arrays in [false, true] {
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
                        arrays,
                        context: false,
                    },
                    cacheability: ExtensionCacheability::Deterministic,
                },
                |_: &mut dyn ExtensionHost| -> ExtensionResult<ExtensionOutput> {
                    panic!("classification never executes providers")
                },
            )
            .unwrap();
        let policy = PreparationPolicy::with_extensions(FormulaParsePolicy::default(), &registry);
        let mut compiler = FormulaCompiler::new();
        for source in [
            "CUSTOM(A1)",
            "CUSTOM(B2)",
            "CUSTOM(SEQUENCE(2))",
            "CUSTOM(SEQUENCE(7))",
            "CUSTOM(unresolved_name)",
            "SUM(CUSTOM(A1))",
            "SUM(CUSTOM(B2))",
            "SUM(CUSTOM(SEQUENCE(3)))",
            "SUM(CUSTOM(SEQUENCE(8)))",
        ] {
            let expected = FormulaCompiler::new()
                .compile_formula_with_extensions(source, &policy, &registry)
                .spill_shape();
            assert_eq!(
                compiler.spill_shape(source, &policy, Some(&registry)),
                expected,
                "{source}, arrays={arrays}"
            );
        }
    }
}

#[test]
fn restored_compilers_relearn_proofs_without_persisted_authority() {
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    assert_eq!(compiler.spill_shape("SUM(A1:A3)", &policy, None), None);
    let cache = cache_snapshot::CacheSnapshot::capture(&compiler, &policy)
        .unwrap()
        .into_owned();
    let mut restored = cache.restore(&policy).unwrap();
    assert!(restored.templates[0].scalar.is_empty());
    assert_eq!(restored.spill_shape("SUM(A1:A3)", &policy, None), None);
    assert_eq!(restored.spill_shape("SUM(B2:B7)", &policy, None), None);
    assert_eq!(restored.usage().parse_attempts, 0);
    assert_eq!(restored.template_hits(), 1);
    assert_eq!(restored.spill_shape("SUM(C3:C9)", &policy, None), None);
    assert_eq!(restored.pool[0].1.len(), 2);
    check(&mut restored, "C2:D8", &policy);
}

#[test]
fn registry_mismatch_never_reuses_a_scalar_proof() {
    let registry = crate::eval::functions::extensions::ExtensionRegistry::new();
    let policy = PreparationPolicy::with_extensions(FormulaParsePolicy::default(), &registry);
    let mut compiler = FormulaCompiler::new();
    assert_eq!(
        compiler.spill_shape("SUM(A1)", &policy, Some(&registry)),
        None
    );
    assert_eq!(
        compiler.spill_shape("SUM(A2)", &policy, None),
        Some(ShapeBounds::UNKNOWN)
    );
    assert_eq!(
        compiler.spill_shape("SUM(A2)", &policy, Some(&registry)),
        None
    );
    let mut invalid = policy.clone();
    invalid.registry.builtin_contract_version += 1;
    assert_eq!(
        compiler.spill_shape("SUM(A3)", &invalid, Some(&registry)),
        None
    );
    assert_eq!(compiler.pool.len(), 2);
}
