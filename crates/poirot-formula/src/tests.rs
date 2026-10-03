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
