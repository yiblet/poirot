use super::*;
/// Parse fixture syntax, failing the test on invalid input.
pub(super) fn parse(text: &str) -> Expr {
    parse_formula(text).unwrap()
}
/// Build an expected binary AST node for parser assertions.
pub(super) fn binary(expr: &Expr, expected: BinaryOp) -> (&Expr, &Expr) {
    if let Expr::Binary { op, left, right } = expr {
        assert_eq!(*op, expected);
        (left, right)
    } else {
        panic!("expected binary, got {expr:?}")
    }
}

#[path = "grammar_tests.rs"]
mod grammar_tests;

#[path = "reference_tests.rs"]
mod reference_tests;

#[path = "robustness_tests.rs"]
mod robustness_tests;

#[test]
fn expression_only_parsing_preserves_trees_and_diagnostics() {
    let default = FormulaParsePolicy::default();
    let bounded = FormulaParsePolicy {
        max_rows: 3,
        max_columns: 3,
        max_expressions: 9,
        ..default
    };
    for policy in [default, bounded] {
        for source in [
            " = SUM(A1,$B$2,,-3)",
            "'Data Set'!B3+Data!RowName",
            "=IF(tRuE,_xlfn.FILTER(A1:B3,C1:C3),\"é\")",
            "A1 (A2)",
            "A4",
            "D1",
            "SUM(A1,",
            "SUM(A1)trailing",
        ] {
            assert_eq!(
                parse_expression_with_policy(source, policy),
                parse_formula_with_policy(source, policy).map(ParsedFormula::into_expression),
                "{source}",
            );
        }
    }
}
