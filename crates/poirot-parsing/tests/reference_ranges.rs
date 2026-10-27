use poirot_parsing::formula::{
    BinaryOp, Expr, FormulaParsePolicy, FormulaPrinter, parse_formula, parse_formula_with_policy,
};

#[test]
fn computed_named_and_grouped_references_roundtrip_without_scalarization() {
    for source in [
        "OFFSET(B3,0,0,-2,-2):C4",
        "A1:INDEX(A:A,3)",
        "Start:End",
        "A1:Z",
        "'Data 2'!Start:'Data 2'!End",
        "IF(TRUE,A1,B1):C4",
        "ABS(1):C4",
        "UNKNOWN(A1):C4",
        "XLOOKUP(A1,B1:B3,C1:C3):D4",
        "OFFSET(A1,0,0):B2:C3",
        "(A1:B3) (B2:C4)",
        "(A1,B1):C4",
        "A1:(B1 C1)",
        "SUM((A1,B1),C1)",
        "SUM (A1)",
        "#REF!:C4",
    ] {
        let expression = parse_formula(source).unwrap_or_else(|e| panic!("{source}: {e:?}"));
        let printed = FormulaPrinter::print(&expression).unwrap();
        assert_eq!(parse_formula(&printed).unwrap(), expression, "{source}");
    }
}

#[test]
fn paired_axes_keep_lexical_priority_but_mixed_rhs_retains_name_identity() {
    for source in ["A:Z", "$A:$Z", "'Data'!A:C", "1:3", "$1:$3"] {
        let Expr::Binary {
            op: BinaryOp::Range,
            left,
            right,
        } = parse_formula(source).unwrap()
        else {
            panic!()
        };
        assert!(matches!(
            (&*left, &*right),
            (Expr::Column { .. }, Expr::Column { .. }) | (Expr::Row { .. }, Expr::Row { .. })
        ));
    }
    let Expr::Binary { left, right, .. } = parse_formula("A1:Z").unwrap() else {
        panic!()
    };
    assert!(matches!(*left, Expr::Cell(_)));
    assert_eq!(*right, Expr::Name("Z".into()));
    assert_eq!(parse_formula("A").unwrap(), Expr::Name("A".into()));
    let Expr::Binary { left, right, .. } = parse_formula("Start:End").unwrap() else {
        panic!()
    };
    assert_eq!(*left, Expr::Name("Start".into()));
    assert_eq!(*right, Expr::Name("End".into()));
    for source in ["A:OFFSET(B1,0,0)", "$A:B1", "1:A1", "A1:3"] {
        assert!(parse_formula(source).is_err(), "{source}");
    }
}

#[test]
fn scalar_operators_and_array_default_calls_do_not_become_range_operands() {
    for source in [
        "(1+2):A1",
        "\"A\":B1",
        "TRUE:A1",
        "{1,2}:A1",
        "TRANSPOSE(A1):B1",
        "MMULT(A1,B1):C1",
        "#VALUE!:A1",
        "A1 ()",
        "A1(A2)",
    ] {
        assert!(parse_formula(source).is_err(), "{source}");
    }
    assert!(matches!(
        parse_formula("A1 (A2)").unwrap(),
        Expr::Binary {
            op: BinaryOp::Intersection,
            ..
        }
    ));
    assert!(matches!(
        parse_formula("LOG10 (100)").unwrap(),
        Expr::Function { .. }
    ));
    assert!(matches!(parse_formula("LOG10").unwrap(), Expr::Cell(_)));
    assert!(matches!(
        parse_formula("UNKNOWN (A2)").unwrap(),
        Expr::Function { .. }
    ));
    assert!(matches!(
        parse_formula("(A1:B3) (B2:C4)").unwrap(),
        Expr::Binary {
            op: BinaryOp::Intersection,
            ..
        }
    ));
}

#[test]
fn dynamic_reference_invocations_retain_original_source_spans_and_admission() {
    let source = "=ROWS(OFFSET(B3,0,0,-2,-2):C4)";
    let parsed = parse_formula_with_policy(source, FormulaParsePolicy::default()).unwrap();
    assert_eq!(parsed.source(), source);
    let invocations = parsed.function_invocations();
    assert_eq!(invocations.len(), 2);
    assert_eq!(
        &source[invocations[0].name_span.start..invocations[0].name_span.end],
        "ROWS"
    );
    assert_eq!(
        &source[invocations[1].name_span.start..invocations[1].name_span.end],
        "OFFSET"
    );
    assert_eq!(invocations[1].argument_count, 5);
    assert_eq!(
        &source[invocations[1].expression_span.start..invocations[1].expression_span.end],
        "OFFSET(B3,0,0,-2,-2)"
    );
    assert!(
        parse_formula_with_policy(
            source,
            FormulaParsePolicy {
                max_expressions: 3,
                ..FormulaParsePolicy::default()
            }
        )
        .is_err()
    );
    assert!(
        parse_formula_with_policy(
            "A1:(B1 C1)",
            FormulaParsePolicy {
                max_depth: 2,
                ..FormulaParsePolicy::default()
            }
        )
        .is_err()
    );
}

#[test]
fn colon_rhs_disambiguates_cells_without_rewriting_sheet_span_atoms() {
    for source in ["S!B2:C5:S!D3", "S!B2:C5:S!D3:A1", "S!B2:(C5:D3)"] {
        let expression = parse_formula(source).unwrap();
        let printed = FormulaPrinter::print(&expression).unwrap();
        assert_eq!(
            parse_formula(&printed).unwrap(),
            expression,
            "{source} -> {printed}"
        );
    }
    let Expr::Binary { left, right, .. } = parse_formula("S!B2:C5:S!D3").unwrap() else {
        panic!()
    };
    assert!(matches!(
        *left,
        Expr::Binary {
            op: BinaryOp::Range,
            ..
        }
    ));
    assert!(matches!(*right, Expr::Cell(_)));
    for source in ["C5:S!D3", "'C5:S'!D3"] {
        let expression = parse_formula(source).unwrap();
        assert!(matches!(&expression, Expr::SheetSpan(_)), "{source}");
        let printed = FormulaPrinter::print(&expression).unwrap();
        assert_eq!(parse_formula(&printed).unwrap(), expression, "{source}");
    }
    for source in ["S!B2:(C5:S!D3)", "S!B2:'C5:S'!D3"] {
        let expression = parse_formula(source).unwrap();
        assert!(
            matches!(&expression,Expr::Binary {right,..} if matches!(&**right,Expr::SheetSpan(_)))
        );
        let printed = FormulaPrinter::print(&expression).unwrap();
        assert_eq!(
            parse_formula(&printed).unwrap(),
            expression,
            "{source} -> {printed}"
        );
    }
}
