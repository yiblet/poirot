use super::*;
#[test]
fn excel_precedence_and_associativity() {
    let expr = parse("2^3^2");
    let (left, right) = binary(&expr, BinaryOp::Power);
    binary(left, BinaryOp::Power);
    assert_eq!(*right, Expr::Number(2.0));
    let expr = parse("-2^2");
    let (left, _) = binary(&expr, BinaryOp::Power);
    assert!(matches!(
        left,
        Expr::Unary {
            op: UnaryOp::Minus,
            ..
        }
    ));
    let expr = parse("2^200%");
    let (_, right) = binary(&expr, BinaryOp::Power);
    assert!(matches!(
        right,
        Expr::Unary {
            op: UnaryOp::Percent,
            ..
        }
    ));
    let expr = parse("1+2*3&\"x\"=\"7x\"");
    let (left, _) = binary(&expr, BinaryOp::Equal);
    let (left, _) = binary(left, BinaryOp::Concat);
    let (_, right) = binary(left, BinaryOp::Add);
    binary(right, BinaryOp::Multiply);
}
#[test]
fn literals_functions_and_missing_arguments() {
    assert_eq!(parse("\"a\"\"b\""), Expr::String("a\"b".into()));
    assert_eq!(parse("true"), Expr::Bool(true));
    assert_eq!(parse("#DIV/0!"), Expr::Error("#DIV/0!".into()));
    assert_eq!(parse("1.25e-2"), Expr::Number(0.0125));
    assert_eq!(
        parse("sum(,1,,)"),
        Expr::Function {
            name: "SUM".into(),
            args: vec![
                Expr::Missing,
                Expr::Number(1.0),
                Expr::Missing,
                Expr::Missing
            ]
        }
    );
    assert_eq!(
        parse("NOW()"),
        Expr::Function {
            name: "NOW".into(),
            args: vec![]
        }
    );
    assert_eq!(parse("my_name"), Expr::Name("my_name".into()));
}
#[test]
fn atp_prefix_spelling_survives_legacy_function_normalization() {
    for name in ["_xlfn.ifna", "_XLFN.IFNA", "_Xlfn.IfNa"] {
        let Expr::Function { name: parsed, .. } = parse(&format!("{name}(1,2)")) else {
            panic!("not a function")
        };
        assert_eq!(parsed, name);
    }
    let Expr::Function { name, .. } = parse("sum(1,2)") else {
        panic!("not a function")
    };
    assert_eq!(name, "SUM");
}

#[test]
fn malformed_leading_brackets_keep_the_expression_error() {
    for (source, position) in [
        ("ROW(['file:///example/Source.ods'#$Data.B3:.D8])", 4),
        ("COLUMN(['file:///example/Source.ods'#$Data.B3])", 7),
    ] {
        let error = parse_formula(source).unwrap_err();
        assert_eq!(error.position, position, "{source}");
        assert_eq!(
            error.reason,
            ParseReasonError::ExpectedExpression,
            "{source}"
        );
    }
    assert!(matches!(parse("[@Amount]"), Expr::Table(_)));
    assert_eq!(
        parse_formula("Sales[@Amount").unwrap_err().reason,
        ParseReasonError::InvalidTableReference,
    );
}

#[test]
fn empty_table_selector_is_short_for_data() {
    for (source, canonical) in [("Sales[]", "Sales[#Data]"), ("[]", "[#Data]")] {
        let expression = parse(source);
        assert_eq!(expression, parse(canonical));
        assert_eq!(FormulaPrinter::print(&expression).unwrap(), canonical);
    }
}
