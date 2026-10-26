//! Independently chosen cases derived from Microsoft's sheet-name grammar:
//! https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/3c420ebb-6ef1-4b0d-959d-76e88c841c3e
//! Sheet-name characters have no leading-letter restriction. No upstream tests reused.
use poirot_parsing::formula::{
    BinaryOp, CellReference, Expr, FormulaLocation, FormulaParsePolicy, ParseErrorKind,
    TranslationPlan, TranslationPolicy, classified, parse_formula, parse_formula_with_policy,
};

#[test]
fn digit_leading_qualifiers_use_the_existing_reference_grammar() {
    for (source, sheet) in [
        ("2024!A1", "2024"),
        ("1Q!A1", "1Q"),
        ("1e3!A1", "1e3"),
        ("1E999!A1", "1E999"),
        ("1.5!A1", "1.5"),
        ("1年!A1", "1年"),
        ("'2024'!A1", "2024"),
        ("Sheet1!A1", "Sheet1"),
    ] {
        assert_eq!(
            parse_formula(source).unwrap(),
            Expr::Cell(CellReference {
                row: 0,
                column: 0,
                row_absolute: false,
                column_absolute: false,
                sheet: Some(sheet.into()),
            }),
            "{source}"
        );
    }
    assert!(matches!(parse_formula("1Q!$B$2").unwrap(), Expr::Cell(c)
        if c.row == 1 && c.column == 1 && c.row_absolute && c.column_absolute));
    assert_eq!(
        parse_formula("2024!Rate").unwrap(),
        Expr::QualifiedName {
            sheet: "2024".into(),
            name: "Rate".into()
        }
    );
    for source in ["2024!A1:B2", "1Q!1:3", "2024!A:C", "1:3", "1Q:2024!A1"] {
        assert!(parse_formula(source).is_ok(), "{source}");
    }
    assert!(
        matches!(parse_formula("2024:2025!A1").unwrap(), Expr::SheetSpan(s)
        if s.first_sheet == "2024" && s.last_sheet == "2025")
    );
    assert!(matches!(
        parse_formula("A1 2024!A1").unwrap(),
        Expr::Binary {
            op: BinaryOp::Intersection,
            ..
        }
    ));
}

#[test]
fn numbers_and_invalid_qualifiers_keep_their_boundaries() {
    for (source, expected) in [
        ("2024", 2024.),
        ("1e3", 1000.),
        ("1e+3", 1000.),
        ("1e-3", 0.001),
        ("1.5", 1.5),
        (".5", 0.5),
        ("1.", 1.),
    ] {
        assert_eq!(parse_formula(source).unwrap(), Expr::Number(expected));
    }
    for source in [
        "1Q",
        "1e",
        "1E999",
        "2024!",
        "1Q!A0",
        "2024!XFE1",
        "2024!A1048577",
        "2024!A1!B2",
        "2024!Rate()",
        "2024!A1tail!",
        "[1]2024!A1",
        "'[1]2024:2025'!A1",
        "1e+3!A1",
        "1Q !A1",
        "A1 2024",
        "A1 2",
    ] {
        assert!(parse_formula(source).is_err(), "{source}");
    }
    for (source, position) in [("2024!", 5), ("2024!A1!B2", 8), ("1Q !A1", 1)] {
        assert_eq!(
            parse_formula(source).unwrap_err().position,
            position,
            "{source}"
        );
    }
    let policy = FormulaParsePolicy {
        max_rows: 1,
        max_columns: 1,
        ..Default::default()
    };
    for source in ["2024!A2", "1Q!B1"] {
        assert_eq!(
            parse_formula_with_policy(source, policy).unwrap_err().kind,
            ParseErrorKind::ResourceLimit
        );
    }
    let policy = FormulaParsePolicy {
        max_expressions: 1,
        ..Default::default()
    };
    assert_eq!(
        parse_formula_with_policy("SUM(2024!A1)", policy)
            .unwrap_err()
            .kind,
        ParseErrorKind::ResourceLimit
    );
}

#[test]
fn spans_translation_and_classification_retain_qualifier_spelling() {
    let source = "=SUM(1年!A1,1Q!$B$2)";
    let parsed = parse_formula_with_policy(source, FormulaParsePolicy::default()).unwrap();
    assert_eq!(parsed.source(), source);
    for span in parsed.expression_spans() {
        assert!(source.get(span.start..span.end).is_some());
    }
    let slices: Vec<_> = parsed
        .expression_spans()
        .iter()
        .map(|span| &source[span.start..span.end])
        .collect();
    assert_eq!(slices, ["1年!A1", "1Q!$B$2", "SUM(1年!A1,1Q!$B$2)"]);
    let calls = parsed.function_invocations();
    assert_eq!(
        &source[calls[0].name_span.start..calls[0].name_span.end],
        "SUM"
    );
    assert_eq!(calls[0].argument_count, 2);
    let plan = TranslationPlan::new(
        source,
        FormulaLocation { row: 0, column: 0 },
        FormulaLocation { row: 1, column: 1 },
        TranslationPolicy::default(),
    )
    .unwrap();
    assert_eq!(
        parse_formula(plan.replacement()).unwrap(),
        parse_formula("SUM('1年'!B2,'1Q'!$B$2)").unwrap()
    );
    assert!(
        TranslationPlan::new(
            "2024!A1",
            FormulaLocation { row: 1, column: 1 },
            FormulaLocation { row: 0, column: 0 },
            TranslationPolicy::default()
        )
        .is_err()
    );
    for source in ["2024!A1", "SUM(1Q!$B$2)", "1e3!A1+1e3", "1年!A1"] {
        let tokens = classified::Tokens::new(source)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(tokens.iter().map(|t| t.raw).collect::<String>(), source);
        assert!(matches!(
            tokens.iter().find(|t| t.raw.ends_with('!')).unwrap().kind,
            classified::TokenKind::Literal
        ));
    }
    assert_eq!(
        classified::normalized_equal("SUM(2024!A1)", "SUM(2024!B2)"),
        Ok(true)
    );
    assert_eq!(
        classified::normalized_equal("SUM(2024!A1)", "SUM(2025!B2)"),
        Ok(false)
    );
}
