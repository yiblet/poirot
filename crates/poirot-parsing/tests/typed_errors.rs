//! Public failure contracts retain categories, positions, evidence and causes.
use poirot_parsing::formula::{
    Expr, FormulaLocation, FormulaParsePolicy, FormulaPrinter, ParseError, ParseErrorKind,
    ParseReasonError, TranslationPlan, TranslationPolicy, parse_formula, parse_formula_with_policy,
};
use std::error::Error;

#[test]
fn syntax_rejections_expose_exact_categories_and_error_causes() {
    for (text, expected) in [
        ("1)", ParseReasonError::TrailingInput),
        ("\"unterminated", ParseReasonError::UnterminatedQuote),
        ("#UNKNOWN!", ParseReasonError::UnknownErrorLiteral),
        ("1e9999", ParseReasonError::NonfiniteNumber),
        (".", ParseReasonError::InvalidNumber),
        ("1+)", ParseReasonError::ExpectedExpression),
        ("'A'!B!C1", ParseReasonError::RepeatedQualifier),
        ("'A'!FOO()", ParseReasonError::InvalidFunctionName),
        ("$BAD$", ParseReasonError::InvalidAbsoluteReference),
        ("'A'!1048577", ParseReasonError::InvalidSheetReference),
        ("A:B!", ParseReasonError::ExpectedSpanCell),
        ("First:Last!Named", ParseReasonError::UnboundedSpanReference),
        ("{A1}", ParseReasonError::NonconstantArray),
        ("{1,2;3}", ParseReasonError::RaggedArray),
        ("A:1", ParseReasonError::IncompatibleRangeEndpoints),
        ("A1:1.5", ParseReasonError::InvalidRangeEndpoint),
        (
            "First:Last!A1:B",
            ParseReasonError::IncompatibleSpanEndpoints,
        ),
        ("'[book]A:B'!A1", ParseReasonError::ExternalSheetSpan),
        ("XFE1", ParseReasonError::CellOutsideGrid),
        (
            "SUM(1",
            ParseReasonError::ExpectedCharacter { expected: ',' },
        ),
    ] {
        let error = parse_formula(text).expect_err(text);
        assert_eq!(error.kind(), ParseErrorKind::Syntax, "{text}");
        assert_eq!(error.reason, expected, "{text}");
        assert_eq!(
            error.source().unwrap().downcast_ref::<ParseReasonError>(),
            Some(&expected),
            "{text}"
        );
    }
}

#[test]
fn resource_and_policy_errors_retain_actual_bounds() {
    let policy = FormulaParsePolicy {
        max_expressions: 2,
        ..FormulaParsePolicy::default()
    };
    let error = parse_formula_with_policy("1+2+3", policy).unwrap_err();
    assert_eq!(error.kind(), ParseErrorKind::ResourceLimit);
    assert_eq!(
        error.reason,
        ParseReasonError::ExpressionLimit {
            limit: 2,
            admitted: 2
        }
    );
    let policy = FormulaParsePolicy {
        max_depth: 2,
        ..FormulaParsePolicy::default()
    };
    assert_eq!(
        parse_formula_with_policy("((1))", policy)
            .unwrap_err()
            .reason,
        ParseReasonError::NestingLimit { limit: 2, depth: 2 }
    );
    let policy = FormulaParsePolicy {
        max_rows: 0,
        ..policy
    };
    let error = parse_formula_with_policy("1", policy).unwrap_err();
    assert_eq!(error.kind(), ParseErrorKind::InvalidPolicy);
    assert_eq!(error.reason, ParseReasonError::InvalidPolicy { policy });
    fn thread_safe<T: Send + Sync + 'static>() {}
    thread_safe::<ParseError>();
}

#[test]
fn printing_and_translation_rejections_preserve_evidence() {
    for (expr, expected) in [
        (
            Expr::Number(f64::INFINITY),
            ParseReasonError::NonfiniteNumber,
        ),
        (
            Expr::Name("A1".into()),
            ParseReasonError::UnrepresentableAst,
        ),
        (
            Expr::Column {
                column: 16384,
                absolute: false,
                sheet: None,
            },
            ParseReasonError::ColumnOutsideGrid { column: 16384 },
        ),
        (
            Expr::Row {
                row: u32::MAX,
                absolute: false,
                sheet: None,
            },
            ParseReasonError::RowOverflow,
        ),
    ] {
        assert_eq!(FormulaPrinter::print(&expr).unwrap_err().reason, expected);
    }
    let origin = FormulaLocation { row: 0, column: 0 };
    let target = FormulaLocation { row: 0, column: 1 };
    let error =
        TranslationPlan::new("XFD1", origin, target, TranslationPolicy::default()).unwrap_err();
    assert_eq!(
        error.reason,
        ParseReasonError::TranslationOutsideGrid {
            coordinate: 16383,
            translated: 16384,
            bound: 16384
        }
    );
    let plan = TranslationPlan::new("A1", origin, target, TranslationPolicy::default()).unwrap();
    assert_eq!(
        plan.verify_source("A2").unwrap_err().reason,
        ParseReasonError::StaleTranslationSource
    );
    assert_eq!(
        TranslationPlan::new(
            "A1:$A$2",
            origin,
            FormulaLocation { row: 0, column: 2 },
            TranslationPolicy::default()
        )
        .unwrap_err()
        .reason,
        ParseReasonError::TranslationReordersEndpoints
    );
}

#[test]
fn bounded_printing_and_reference_policies_expose_the_rejected_limits() {
    let policy = FormulaParsePolicy {
        max_rows: 3,
        max_columns: 4,
        ..FormulaParsePolicy::default()
    };
    assert_eq!(
        parse_formula_with_policy("E1", policy).unwrap_err().reason,
        ParseReasonError::ConfiguredGrid {
            max_rows: 3,
            max_columns: 4
        }
    );
    let mut expression = Expr::Number(1.0);
    for _ in 0..128 {
        expression = Expr::Unary {
            op: poirot_parsing::formula::UnaryOp::Plus,
            operand: Box::new(expression),
        };
    }
    assert_eq!(
        FormulaPrinter::print(&expression).unwrap_err().reason,
        ParseReasonError::PrinterLimit {
            depth: 128,
            nodes: 129
        }
    );
    let location = FormulaLocation {
        row: 1048576,
        column: 0,
    };
    assert_eq!(
        TranslationPlan::new(
            "A1",
            location,
            FormulaLocation { row: 0, column: 0 },
            TranslationPolicy::default()
        )
        .unwrap_err()
        .reason,
        ParseReasonError::TranslationLocationOutsideGrid {
            row: 1048576,
            column: 0,
            max_rows: 1048576,
            max_columns: 16384
        }
    );
    let mut span = match parse_formula("First:Last!A1").unwrap() {
        Expr::SheetSpan(span) => span,
        _ => panic!("expected sheet span"),
    };
    span.first_sheet.clear();
    assert_eq!(
        FormulaPrinter::print(&Expr::SheetSpan(span))
            .unwrap_err()
            .reason,
        ParseReasonError::InvalidSheetSpan
    );
}
