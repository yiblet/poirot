use super::*;

fn policy(max_expressions: usize) -> FormulaParsePolicy {
    FormulaParsePolicy {
        max_expressions,
        ..FormulaParsePolicy::default()
    }
}

fn complexity_error(source: &str, limit: usize, position: usize) {
    let error = parse_formula_with_policy(source, policy(limit)).unwrap_err();
    assert!(matches!(
        error.reason,
        super::ParseReasonError::ExpressionLimit { .. }
    ));
    assert_eq!(error.position, position);
}

#[test]
fn missing_operands_share_the_expression_admission_boundary() {
    for (source, required, rejected_position) in [
        ("SUM(,)", 3, 5),
        ("SUM(,,)", 4, 6),
        ("SUM(,1,,)", 5, 8),
        ("SUM(1,,2)", 4, 7),
        ("SUM(,SUM(,),)", 6, 12),
    ] {
        assert!(parse_formula_with_policy(source, policy(required)).is_ok());
        complexity_error(source, required - 1, rejected_position);
    }
    complexity_error("SUM(,)", 1, 4);
    let empty = parse_formula_with_policy("NOW()", policy(1)).unwrap();
    assert!(matches!(empty.expression(), Expr::Function { args, .. } if args.is_empty()));
    assert_eq!(empty.function_invocations()[0].argument_count, 0);
}

#[test]
fn canonical_bound_applies_to_missing_operands_without_function_arity_guessing() {
    let accepted = format!("SUM({})", ",".repeat(1022));
    let parsed = parse_formula(&accepted).unwrap();
    assert!(matches!(parsed, Expr::Function { args, .. }
        if args.len() == 1023 && args.iter().all(|arg| matches!(arg, Expr::Missing))));
    let rejected = format!("SUM({})", ",".repeat(1023));
    complexity_error(&rejected, 1024, 1027);
    let large = format!("SUM({})", ",".repeat(100_000));
    complexity_error(&large, 3, 6);
}

#[test]
fn omitted_arguments_keep_ast_invocations_and_expression_spans() {
    let source = "=sum(,SUM(,),1,,)";
    let parsed = parse_formula_with_policy(source, policy(8)).unwrap();
    assert_eq!(parsed.source(), source);
    assert_eq!(parsed.policy(), policy(8));
    assert_eq!(
        parsed.expression(),
        &Expr::Function {
            name: "SUM".into(),
            args: vec![
                Expr::Missing,
                Expr::Function {
                    name: "SUM".into(),
                    args: vec![Expr::Missing, Expr::Missing]
                },
                Expr::Number(1.0),
                Expr::Missing,
                Expr::Missing,
            ],
        }
    );
    assert_eq!(
        parsed.function_invocations(),
        &[
            FunctionInvocation {
                name_span: FormulaSpan { start: 1, end: 4 },
                expression_span: FormulaSpan { start: 1, end: 17 },
                argument_count: 5,
            },
            FunctionInvocation {
                name_span: FormulaSpan { start: 6, end: 9 },
                expression_span: FormulaSpan { start: 6, end: 12 },
                argument_count: 2,
            },
        ]
    );
    assert_eq!(
        parsed.expression_spans(),
        &[
            FormulaSpan { start: 6, end: 12 },
            FormulaSpan { start: 13, end: 14 },
            FormulaSpan { start: 1, end: 17 },
        ]
    );
}

#[test]
fn rejected_omission_preserves_outer_metadata_checkpoint_and_depth_cleanup() {
    let mut parser = Parser {
        input: "SUM(OK(),,)",
        pos: 0,
        depth: 0,
        expressions: 0,
        policy: policy(3),
        spans: Vec::new(),
        function_invocations: Vec::new(),
    };
    let error = parser.expression(0, true).unwrap_err();
    assert!(matches!(
        error.reason,
        super::ParseReasonError::ExpressionLimit { .. }
    ));
    assert_eq!(error.position, 10);
    assert_eq!(parser.expressions, 3);
    assert_eq!(parser.depth, 0);
    assert!(parser.function_invocations.is_empty());
    assert_eq!(parser.spans, [FormulaSpan { start: 4, end: 8 }]);
}
