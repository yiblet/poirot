use super::*;

fn parsed(source: &str) -> ParsedFormula {
    parse_formula_with_policy(source, FormulaParsePolicy::default()).unwrap()
}
fn names(parsed: &ParsedFormula) -> Vec<&str> {
    parsed
        .function_invocations()
        .iter()
        .map(|call| {
            assert!(call.name_span.start >= call.expression_span.start);
            assert!(call.name_span.end <= call.expression_span.end);
            assert!(
                parsed
                    .source()
                    .get(call.expression_span.start..call.expression_span.end)
                    .is_some()
            );
            &parsed.source()[call.name_span.start..call.name_span.end]
        })
        .collect()
}
#[test]
fn source_names_are_attached_to_real_nested_nodes_without_normalization_changes() {
    let syntax = parsed("IFERROR(edate(1,2),yearfrac(3,4))+edate(5,6)");
    assert_eq!(names(&syntax), ["IFERROR", "edate", "yearfrac", "edate"]);
    assert_eq!(
        syntax
            .function_invocations()
            .iter()
            .map(|call| call.argument_count)
            .collect::<Vec<_>>(),
        [2, 2, 2, 2]
    );
    assert!(
        syntax
            .function_invocations()
            .windows(2)
            .all(|pair| pair[0].name_span.start < pair[1].name_span.start)
    );
    let Expr::Binary { left, .. } = syntax.expression() else {
        panic!("binary root");
    };
    let Expr::Function { args, .. } = left.as_ref() else {
        panic!("function root");
    };
    assert!(matches!(&args[0], Expr::Function { name, .. } if name == "EDATE"));
}
#[test]
fn escaped_strings_quoted_sheet_names_and_utf8_prefixes_do_not_fabricate_calls() {
    let syntax = parsed(r#"IFERROR("edate(""1"",2)",'YEARFRAC(2,3)'!A1)"#);
    assert_eq!(names(&syntax), ["IFERROR"]);
    let syntax = parsed(r#""😀"&edate(1,2)"#);
    assert_eq!(names(&syntax), ["edate"]);
    assert_eq!(
        syntax.function_invocations()[0].name_span.start,
        "\"😀\"&".len()
    );
}
#[test]
fn failed_outer_parse_discards_successful_inner_invocations() {
    let mut parser = Parser {
        input: "SUM(edate(1,2),BROKEN(",
        pos: 0,
        depth: 0,
        expressions: 0,
        policy: FormulaParsePolicy::default(),
        spans: Vec::new(),
        function_invocations: Vec::new(),
    };
    assert!(parser.expression(0, true).is_err());
    assert!(parser.function_invocations.is_empty());
    assert!(parse_formula_with_policy("Data!edate(1,2)", FormulaParsePolicy::default()).is_err());
    assert_eq!(names(&parsed("edate(1,2)")), ["edate"]);
}
