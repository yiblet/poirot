use super::*;
#[test]
fn sheet_span_whole_axes_keep_source_and_legal_bounds() {
    for (source, first, last) in [
        ("VLOOKUP(7,First:Last!$A:$B,2,FALSE)", (0, 0), (1048575, 1)),
        ("HLOOKUP(7,First:Last!$1:$3,2,FALSE)", (0, 0), (2, 16383)),
    ] {
        let parsed = parse_formula_with_policy(source, FormulaParsePolicy::default()).unwrap();
        assert_eq!(parsed.source(), source);
        let Expr::Function { args, .. } = parsed.expression() else {
            panic!()
        };
        let Expr::SheetSpan(span) = &args[1] else {
            panic!()
        };
        let end = span.end.as_ref().unwrap();
        assert_eq!((span.start.row, span.start.column), first);
        assert_eq!((end.row, end.column), last);
        assert!(span.start.row_absolute && span.start.column_absolute);
        assert!(end.row_absolute && end.column_absolute);
        assert_eq!(parsed.function_invocations().len(), 1);
        let call = &parsed.function_invocations()[0];
        assert_eq!(
            &source[call.name_span.start..call.name_span.end],
            &source[..7]
        );
        assert_eq!(
            &source[call.expression_span.start..call.expression_span.end],
            source
        );
    }
    for source in [
        "First:Last!A",
        "First:Last!1",
        "First:Last!A:2",
        "First:Last!XFE:XFE",
        "First:Last!1:1048577",
    ] {
        assert!(
            parse_formula_with_policy(source, FormulaParsePolicy::default()).is_err(),
            "{source}"
        );
    }
}
#[test]
fn references_and_sheet_escaping() {
    assert_eq!(
        parse("='O''Brien Data'!$XFD$1048576"),
        Expr::Cell(CellReference {
            row: 1048575,
            column: 16383,
            row_absolute: true,
            column_absolute: true,
            sheet: Some("O'Brien Data".into())
        })
    );
    let expr = parse("Sheet1!A1:B2");
    let (_, right) = binary(&expr, BinaryOp::Range);
    assert!(matches!(right, Expr::Cell(CellReference { sheet: Some(s), .. }) if s == "Sheet1"));
    for expression in ["A:C", "$A:$C", "1:3", "$1:$3", "Sheet1!A:C"] {
        binary(&parse(expression), BinaryOp::Range);
    }
}
#[test]
fn reference_operators_and_arrays() {
    let expr = parse("A1:B5 B2:C6,D1");
    let (left, _) = binary(&expr, BinaryOp::Union);
    let (left, right) = binary(left, BinaryOp::Intersection);
    binary(left, BinaryOp::Range);
    binary(right, BinaryOp::Range);
    assert!(matches!(parse("SUM((A1,B1),C1)"), Expr::Function { args, .. } if args.len() == 2));
    assert_eq!(
        parse("{1,\"two\";TRUE,#N/A}"),
        Expr::Array(vec![
            vec![Expr::Number(1.0), Expr::String("two".into())],
            vec![Expr::Bool(true), Expr::Error("#N/A".into())]
        ])
    );
    assert_eq!(parse("  = 1 + 2  "), parse("1+2"));
}
