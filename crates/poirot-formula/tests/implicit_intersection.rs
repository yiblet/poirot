use poirot_formula::{BinaryOp, Expr, FormulaPrinter, UnaryOp, parse_formula};

fn select(operand: Expr) -> Expr {
    Expr::Unary {
        op: UnaryOp::ImplicitIntersection,
        operand: Box::new(operand),
    }
}

#[test]
fn explicit_selection_includes_ranges_but_does_not_consume_arithmetic() {
    let range = parse_formula("A1:A3").unwrap();
    assert_eq!(parse_formula("@A1:A3").unwrap(), select(range.clone()));
    assert_eq!(
        parse_formula("@A1:A3+2").unwrap(),
        Expr::Binary {
            op: BinaryOp::Add,
            left: Box::new(select(range)),
            right: Box::new(Expr::Number(2.)),
        }
    );
    assert_eq!(
        parse_formula("@@42").unwrap(),
        select(select(Expr::Number(42.)))
    );
    for source in ["@", "SUM(@)", "{@1,2}"] {
        assert!(parse_formula(source).is_err(), "{source}");
    }
}

#[test]
fn explicit_selection_roundtrips_nested_and_grouped_operands() {
    for source in [
        "@A1:A3+2",
        "@@42",
        "@\"text\"",
        "@#N/A",
        "@{1,2;3,4}",
        "@(A1:A3 A2:A4)",
        "@(A1,A2)",
        "@(1+2)",
        "SUM(@A1:A3,2)",
        "(@42)%",
        "@42%",
        "-@A1:A3",
        "@-42",
        "@A:A",
        "@1:3",
    ] {
        let expression = parse_formula(source).unwrap();
        let canonical = FormulaPrinter::print(&expression).unwrap();
        assert_eq!(parse_formula(&canonical).unwrap(), expression, "{source}");
    }
}

#[test]
fn explicit_selection_translation_keeps_the_operator_and_reference_absolutes() {
    use poirot_formula::{FormulaLocation, TranslationPlan, TranslationPolicy};
    let translated = TranslationPlan::new(
        "=@$A1:$A3+@B$1",
        FormulaLocation { row: 0, column: 0 },
        FormulaLocation { row: 1, column: 2 },
        TranslationPolicy::default(),
    )
    .unwrap();
    assert_eq!(translated.replacement(), "@$A2:$A4+@D$1");
    assert_eq!(
        parse_formula(translated.replacement()).unwrap(),
        *translated.expression()
    );
}
