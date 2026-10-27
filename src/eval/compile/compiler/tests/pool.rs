use super::super::compiler_usage::CompilerUsage;
use super::super::formula_compiler::FormulaCompiler;
use super::super::preparation_policy::PreparationPolicy;
use super::super::prepared_formula::PreparedFormula;
use crate::eval::compile::bound;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::session::CellEvaluator;
use crate::model::{CellAddress, Workbook};
use poirot_parsing::formula::FormulaParsePolicy;
use std::sync::Arc;

#[test]
fn bytecode_preparation_does_not_retain_eager_expression_trees() {
    let mut compiler = FormulaCompiler::new();
    let prepared = compiler.compile_formula("IFERROR(A1+B2,3)", &PreparationPolicy::default());
    let PreparedFormula::Ready(formula) = prepared.as_ref() else {
        panic!("valid formula was not prepared")
    };
    assert!(!formula.bytecode().template.code.is_empty());
    assert_eq!(prepared.source(), "IFERROR(A1+B2,3)");
    // Reading exact source identity must not accidentally rehydrate syntax.
    assert_eq!(formula.parsed().unwrap().source(), prepared.source());
    assert!(matches!(
        formula.executable(),
        bound::Expression::Logical(_)
    ));
}

#[test]
fn prepared_evaluation_uses_bytecode_without_materializing_trees() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    let at: CellAddress = "A1".parse().unwrap();
    sheet.set_formula(at, "IF(FALSE,1/0,2+3)").unwrap();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &PreparationPolicy::default());
    let prepared = plan
        .formula(EvaluationTarget {
            sheet: 0,
            address: at,
        })
        .unwrap();
    let PreparedFormula::Ready(_formula) = prepared.as_ref() else {
        panic!("valid formula")
    };
    let mut session = crate::eval::session::Session::with_context(
        &workbook,
        crate::eval::config::EvaluationInputs::default(),
    );
    session.sources.plan = Some(&plan);
    assert_eq!(
        CellEvaluator::new(&mut session).cell(0, at).unwrap(),
        crate::model::CellValue::Number(5.)
    );
}

#[test]
fn indexed_reuse_preserves_exact_source_and_survives_growth() {
    let mut compiler = FormulaCompiler::new();
    let policy = PreparationPolicy::default();
    let first = compiler.compile_formula("1+2", &policy);
    for number in 3..259 {
        compiler.compile_formula(&format!("1+{number}"), &policy);
    }
    assert!(Arc::ptr_eq(
        &first,
        &compiler.compile_formula("1+2", &policy)
    ));
    let spaced = compiler.compile_formula("1 + 2", &policy);
    assert!(!Arc::ptr_eq(&first, &spaced));
    assert_eq!(spaced.source(), "1 + 2");
    assert_eq!(compiler.usage().parse_attempts, 258);
    assert_eq!(compiler.usage().reused_formulas, 1);
}

#[test]
fn policies_separate_successful_plans_and_cached_parse_failures() {
    let mut compiler = FormulaCompiler::new();
    let policy = PreparationPolicy::default();
    let mut limited = policy.clone();
    limited.parse.max_expressions = 1;
    let ready = compiler.compile_formula("1+2", &policy);
    let failed = compiler.compile_formula("1+2", &limited);
    assert!(matches!(ready.as_ref(), PreparedFormula::Ready(_)));
    assert!(matches!(
        failed.as_ref(),
        PreparedFormula::ParseFailure { .. }
    ));
    assert!(Arc::ptr_eq(
        &ready,
        &compiler.compile_formula("1+2", &policy)
    ));
    assert!(Arc::ptr_eq(
        &failed,
        &compiler.compile_formula("1+2", &limited)
    ));
    assert_eq!(
        compiler.usage(),
        &CompilerUsage {
            parse_attempts: 2,
            bind_attempts: 1,
            reused_formulas: 2,
        }
    );
}

#[test]
fn registry_mismatch_cannot_poison_or_reuse_a_valid_pool() {
    let mut compiler = FormulaCompiler::new();
    let registry = crate::eval::functions::extensions::ExtensionRegistry::new();
    let policy = PreparationPolicy::with_extensions(FormulaParsePolicy::default(), &registry);
    let mismatch = compiler.compile_formula("1+2", &policy);
    assert!(matches!(
        mismatch.as_ref(),
        PreparedFormula::RegistryMismatch { .. }
    ));
    let ready = compiler.compile_formula_with_extensions("1+2", &policy, &registry);
    assert!(matches!(ready.as_ref(), PreparedFormula::Ready(_)));
    assert!(matches!(
        compiler.compile_formula("1+2", &policy).as_ref(),
        PreparedFormula::RegistryMismatch { .. }
    ));
    assert!(Arc::ptr_eq(
        &ready,
        &compiler.compile_formula_with_extensions("1+2", &policy, &registry)
    ));
    assert_eq!(
        compiler.usage(),
        &CompilerUsage {
            parse_attempts: 1,
            bind_attempts: 1,
            reused_formulas: 1,
        }
    );
}
