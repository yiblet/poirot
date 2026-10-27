//! Scalar-policy preparation limits remain operational failures only when reached.
use poirot::calculation::EvaluationContext;
use poirot::eval::overlay::Overlay;
use poirot::eval::{
    self as eval, CapabilityId, EvaluationBudgets, EvaluationError, EvaluationInputs,
    EvaluationOutcome, EvaluationTarget,
};
use poirot::{CellValue, Workbook};
use poirot_parsing::formula::{self, FormulaParsePolicy, ParseErrorKind};
fn target(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
#[test]
fn parser_classifies_limits_separately_from_syntax_and_invalid_configuration() {
    assert_eq!(
        formula::parse_formula("SUM(").unwrap_err().kind(),
        ParseErrorKind::Syntax
    );
    assert!(
        formula::parse_formula_with_policy(
            "1+2",
            FormulaParsePolicy {
                max_expressions: 2,
                ..Default::default()
            }
        )
        .is_ok()
    );
    for (source, policy) in [
        (
            "1+2+3",
            FormulaParsePolicy {
                max_expressions: 2,
                ..Default::default()
            },
        ),
        (
            "((1))",
            FormulaParsePolicy {
                max_depth: 2,
                ..Default::default()
            },
        ),
        (
            "A2",
            FormulaParsePolicy {
                max_rows: 1,
                ..Default::default()
            },
        ),
    ] {
        assert_eq!(
            formula::parse_formula_with_policy(source, policy)
                .unwrap_err()
                .kind(),
            ParseErrorKind::ResourceLimit
        );
    }
    assert_eq!(
        formula::parse_formula_with_policy(
            "1",
            FormulaParsePolicy {
                max_depth: 0,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind(),
        ParseErrorKind::InvalidPolicy
    );
}
#[test]
fn scalar_policy_preparation_refusals_abort_iferror_but_lazy_sources_remain_unused() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    let large = format!("SUM({})", vec!["1"; 1100].join(","));
    sheet
        .set_formula(target("A1").address, large.clone())
        .unwrap();
    sheet
        .set_formula(target("B1").address, "IFERROR(A1,42)")
        .unwrap();
    sheet
        .set_formula(target("C1").address, "IF(TRUE,7,A1)")
        .unwrap();
    sheet.set_formula(target("D1").address, "SUM(").unwrap();
    sheet
        .set_formula(target("E1").address, "IFERROR(D1,42)")
        .unwrap();
    let context = EvaluationContext::default();
    // Scalar intersection preserves demand-driven preparation. Dynamic spilling
    // must eagerly resolve formulas whose resource failure leaves their type unknown.
    let policy = poirot::eval::EvaluationPolicy {
        arrays: eval::ArrayResultPolicy::ScalarIntersection,
        ..Default::default()
    };
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(&book)
        .with_policy(policy.clone())
        .build(context.clone())
        .unwrap();
    assert_eq!(direct.cell(target("C1")).unwrap(), CellValue::Number(7.));
    assert_eq!(direct.cell(target("E1")).unwrap(), CellValue::Number(42.));
    assert!(matches!(
        direct.cell(target("B1")),
        Err(EvaluationError::ResourceLimit {
            capability: CapabilityId::FormulaParsing,
            ..
        })
    ));
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy)
        .build()
        .unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C1"))
            .unwrap(),
        CellValue::Number(7.)
    );
    assert!(matches!(
        overlay.evaluation(&context).unwrap().cell(target("B1")),
        Err(EvaluationError::ResourceLimit {
            capability: CapabilityId::FormulaParsing,
            ..
        })
    ));
    assert!(matches!(
        eval::run_formula(
            &book,
            0,
            target("A1").address,
            &large,
            EvaluationInputs::default(),
            EvaluationBudgets::default()
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationError::ResourceLimit {
                capability: CapabilityId::FormulaParsing,
                ..
            },
            ..
        }
    ));
}
