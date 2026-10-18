use poirot_parsing::{
    criteria::{Comparison, Operand, parse_criterion},
    number::{decimal, parse_finite_decimal},
};

#[test]
fn decimal_prefix_and_complete_parsing_preserve_fraction_and_remainder() {
    assert_eq!(decimal("2.5"), Ok(("", "2.5")));
    assert_eq!(decimal("2.5tail"), Ok(("tail", "2.5")));
    assert_eq!(decimal("-.25e+2;next"), Ok((";next", "-.25e+2")));
    assert_eq!(parse_finite_decimal("2.5"), Some(2.5));
    assert_eq!(parse_finite_decimal("2.5tail"), None);
    assert_eq!(parse_finite_decimal("1e"), None);
    assert_eq!(parse_finite_decimal("$1.44"), None);
}

#[test]
fn criteria_are_not_formula_or_formatted_number_text() {
    let syntax = parse_criterion("<>(2)");
    assert_eq!(syntax.comparison, Comparison::Ne);
    assert_eq!(syntax.classic_operand(), Operand::Text("(2)"));
    assert_eq!(
        parse_criterion(">=2.5").classic_operand(),
        Operand::Number(2.5)
    );
    assert_eq!(
        parse_criterion("=SUM(A1:A3)").classic_operand(),
        Operand::Text("SUM(A1:A3)")
    );
    assert_eq!(
        parse_criterion("a~*?").classic_operand(),
        Operand::Text("a~*?")
    );
}
