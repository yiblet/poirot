//! Original syntax tests for the postfix spill reference described by Microsoft:
//! https://support.microsoft.com/en-us/excel/spilled-range-operator
//! Precedence and invalid operand validation are explicit Poirot grammar contracts.
use poirot_parsing::formula::{
    BinaryOp, Expr, FormulaLocation, FormulaPrinter, TranslationPlan, UnaryOp, parse_formula,
};

#[test]
fn postfix_spill_binds_before_range_and_prefix_selection() {
    let Expr::Unary {
        op: UnaryOp::ImplicitIntersection,
        operand,
    } = parse_formula("@A1#").unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        *operand,
        Expr::Unary {
            op: UnaryOp::SpillRange,
            ..
        }
    ));
    let Expr::Binary {
        op: BinaryOp::Range,
        left,
        right,
    } = parse_formula("A1:B2#").unwrap()
    else {
        panic!()
    };
    assert!(matches!(*left, Expr::Cell(_)));
    assert!(matches!(
        *right,
        Expr::Unary {
            op: UnaryOp::SpillRange,
            ..
        }
    ));
    let Expr::Unary {
        op: UnaryOp::SpillRange,
        operand,
    } = parse_formula("(A1:B2)#").unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        *operand,
        Expr::Binary {
            op: BinaryOp::Range,
            ..
        }
    ));
}
#[test]
fn canonical_grouping_preserves_spill_semantics() {
    for formula in [
        "@A1#",
        "-A1#",
        "(A1+B1)#",
        "A1#:D5",
        "A1:B2#",
        "(A1:B2)#",
        "A1#%",
        "A1%#",
        "SUM('Quoted sheet'!$A1#)",
        "A1##",
    ] {
        let expression = parse_formula(formula).unwrap();
        let printed = FormulaPrinter::print(&expression).unwrap();
        assert_eq!(
            parse_formula(&printed).unwrap(),
            expression,
            "{formula}: {printed}"
        );
    }
}
#[test]
fn translation_preserves_absolute_axes_and_quoted_sheets() {
    let plan = TranslationPlan::new(
        "SUM('Quoted sheet'!$A1#,B$2#)",
        FormulaLocation { row: 0, column: 0 },
        FormulaLocation { row: 2, column: 3 },
        Default::default(),
    )
    .unwrap();
    assert_eq!(plan.replacement(), "SUM('Quoted sheet'!$A3#,E$2#)");
}
