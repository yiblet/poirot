use super::super::formula_compiler::FormulaCompiler;
use super::super::preparation_policy::PreparationPolicy;
use super::super::scalar_locality::ScalarLocality;
use super::super::scalar_matcher::ScalarMatcher;
use super::super::templates::scalar_classification_key;
use poirot_parsing::formula::FormulaParsePolicy;

/// Build a test matcher from spans produced by the authoritative key pass.
fn scalar_matcher(source: &str) -> Option<ScalarMatcher> {
    let mut key = Vec::new();
    let mut cells = Vec::new();
    if !matches!(
        scalar_classification_key(source, &PreparationPolicy::default(), &mut key, &mut cells),
        Some(ScalarLocality::Eligible(_))
    ) {
        return None;
    }
    let mut matcher = ScalarMatcher::default();
    matcher.update(source, &cells).then_some(matcher)
}

#[test]
fn promotion_reuses_buffers_and_declined_updates_preserve_the_old_pattern() {
    let policy = PreparationPolicy::default();
    let mut matcher = scalar_matcher("SUM(A1:A2)").unwrap();
    matcher.source.reserve(256);
    matcher.cells.reserve(8);
    let source_buffer = matcher.source.as_ptr();
    let cells_buffer = matcher.cells.as_ptr();
    let source = "IFERROR(COUNT($B3:C$4),0)";
    let mut key = Vec::new();
    let mut cells = Vec::new();
    assert!(matches!(
        scalar_classification_key(source, &policy, &mut key, &mut cells),
        Some(ScalarLocality::Eligible(_))
    ));
    assert!(matcher.update(source, &cells));
    assert_eq!(matcher.source.as_ptr(), source_buffer);
    assert_eq!(matcher.cells.as_ptr(), cells_buffer);
    assert!(matcher.matches("IFERROR(COUNT(D5:$E6),0)", &policy));
    assert!(!matcher.matches("SUM(A1:A2)", &policy));
    let excessive = "x".repeat(ScalarMatcher::MAX_SOURCE_BYTES + 1);
    assert!(!matcher.update(&excessive, &[]));
    assert!(matcher.matches("IFERROR(COUNT(D5:$E6),0)", &policy));
    assert!(!matcher.update("A1", &vec![(0, 2); ScalarMatcher::MAX_CELLS + 1]));
    assert!(matcher.matches("IFERROR(COUNT(D5:$E6),0)", &policy));
}

#[test]
fn exact_gaps_and_canonical_cells_reject_syntax_changes() {
    let policy = PreparationPolicy::default();
    for (pattern, matching) in [
        ("A1", "$B$2"),
        ("SUM($A1,A$2)", "SUM(B$3,$C4)"),
        (
            "SUM('O''Brien'!A1:B2,\"a\"\"b\")",
            "SUM('O''Brien'!$C$3:D4,\"a\"\"b\")",
        ),
        ("SUM(A1,'S'!$B:$C)", "SUM(B2,'S'!$B:$C)"),
        ("SUM(A1 (A2))", "SUM(B3 (C4))"),
    ] {
        let matcher = scalar_matcher(pattern).unwrap();
        assert!(matcher.matches(matching, &policy), "{pattern}: {matching}");
        assert!(!matcher.matches(&format!("{matching}+SEQUENCE(2)"), &policy));
        assert!(!matcher.matches(&format!("{matching}garbage"), &policy));
    }
    let matcher = scalar_matcher("SUM(A1,'S'!$B:$C,\"text\")").unwrap();
    for source in [
        "SUM(A1048577,'S'!$B:$C,\"text\")",
        "SUM(XFE1,'S'!$B:$C,\"text\")",
        "SUM(A:A,'S'!$B:$C,\"text\")",
        "SUM(LOG10(A1),'S'!$B:$C,\"text\")",
        "SUM(A2,'Other'!$B:$C,\"text\")",
        "SUM(A2,'S'!$B:$D,\"text\")",
        "SUM(A2,'S'!$B:$C,\"different\")",
        "sum(A2,'S'!$B:$C,\"text\")",
        "SUM(A2,'S'!$B:$C,\"text\"))",
    ] {
        assert!(!matcher.matches(source, &policy), "{source}");
    }
    let matcher = scalar_matcher("SUM(A1)").unwrap();
    let limited = PreparationPolicy {
        parse: FormulaParsePolicy {
            max_rows: 4,
            max_columns: 4,
            ..policy.parse
        },
        ..policy
    };
    assert!(!matcher.matches("SUM(A5)", &limited));
    assert!(!matcher.matches("SUM(E1)", &limited));
}

#[test]
fn promoted_scalar_sources_still_match_authoritative_inference() {
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    for row in [1, 2, 3, 127, 4, 16, 7] {
        let source = format!("IFERROR(VLOOKUP(A{row},'S'!$B:$C,2,FALSE),\"missing\")");
        let expected = FormulaCompiler::new()
            .compile_formula(&source, &policy)
            .spill_shape();
        assert_eq!(compiler.spill_shape(&source, &policy, None), expected);
    }
    assert!(compiler.templates[0].last_scalar.is_some());
    assert_eq!(compiler.pool[0].1.len(), 1);
    for source in [
        "IFERROR(VLOOKUP(A0,'S'!$B:$C,2,FALSE),\"missing\")",
        "IFERROR(VLOOKUP(A2,'S'!$B:$C,2,FALSE),SEQUENCE(3))",
        "SEQUENCE(3)",
        "SEQUENCE(8)",
    ] {
        let expected = FormulaCompiler::new()
            .compile_formula(source, &policy)
            .spill_shape();
        assert_eq!(compiler.spill_shape(source, &policy, None), expected);
    }
}

#[test]
fn alternating_classes_and_unique_literals_do_not_rebuild_patterns() {
    let policy = PreparationPolicy::default();
    let mut alternating = FormulaCompiler::new();
    let mut unique = FormulaCompiler::new();
    for row in 1..=128 {
        let function = if row % 2 == 0 { "COUNT" } else { "SUM" };
        assert_eq!(
            alternating.spill_shape(&format!("{function}(A{row})"), &policy, None),
            None
        );
        assert_eq!(
            unique.spill_shape(&format!("SUM(A{row},\"text {row}\",{row})"), &policy, None),
            None
        );
        assert!(alternating.templates[0].last_scalar.is_none());
        assert!(unique.templates[0].last_scalar.is_none());
    }
    assert_eq!(alternating.usage().parse_attempts, 2);
    assert_eq!(unique.usage().parse_attempts, 1);
}

#[test]
fn matcher_storage_limits_decline_optimization_without_changing_types() {
    let source = format!("SUM(\"{}\")", "x".repeat(16 * 1024));
    assert!(scalar_matcher(&source).is_none());
    let source = format!(
        "SUM({})",
        (1..=65)
            .map(|row| format!("A{row}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    assert!(scalar_matcher(&source).is_none());
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    for source in [
        source.clone(),
        source.replace('A', "B"),
        source.replace('A', "C"),
    ] {
        assert_eq!(compiler.spill_shape(&source, &policy, None), None);
        assert!(compiler.templates[0].last_scalar.is_none());
        assert!(compiler.templates[0].scalar_probation.is_none());
    }
}

#[test]
fn promoted_matchers_cannot_cross_registry_or_parser_policy() {
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    for source in ["SUM(A1)", "SUM(A2)"] {
        assert_eq!(compiler.spill_shape(source, &policy, None), None);
    }
    assert!(compiler.templates[0].last_scalar.is_some());
    let registry = crate::eval::functions::extensions::ExtensionRegistry::new();
    assert_eq!(
        compiler.spill_shape("SUM(A3)", &policy, Some(&registry)),
        Some(crate::model::ShapeBounds::UNKNOWN)
    );
    let limited = PreparationPolicy {
        parse: FormulaParsePolicy {
            max_expressions: 1,
            ..policy.parse
        },
        ..policy
    };
    let expected = FormulaCompiler::new()
        .compile_formula("SUM(A3)", &limited)
        .spill_shape();
    assert_eq!(compiler.spill_shape("SUM(A3)", &limited, None), expected);
}
