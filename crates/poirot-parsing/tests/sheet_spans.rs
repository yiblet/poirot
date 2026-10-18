use poirot_parsing::formula::{
    Expr, FormulaParsePolicy, FormulaPrinter, parse_formula, parse_formula_with_policy,
};
#[test]
fn bounded_sheet_spans_are_typed_and_round_trip() {
    for formula in [
        "Sheet1:Sheet3!A1",
        "Sheet1:Sheet3!$A$1:B2",
        "'First Data:Last Data'!A1:B2",
        "'O''Brien:Last Data'!$B1:A$2",
        "SUM(Sheet1:Sheet3!A1:B2,7)",
        "SUM('O''Brien:Last Data'!A1)",
    ] {
        let expr = parse_formula(formula).unwrap();
        let printed = FormulaPrinter::print(&expr).unwrap();
        assert_eq!(parse_formula(&printed).unwrap(), expr, "{formula}");
    }
    let Expr::SheetSpan(span) = parse_formula("Sheet1:Sheet3!$B1:A$2").unwrap() else {
        panic!("expected span")
    };
    assert_eq!(span.first_sheet, "Sheet1");
    assert_eq!(span.last_sheet, "Sheet3");
    assert_eq!(span.start.column, 1);
    assert!(span.start.column_absolute);
    assert!(span.end.unwrap().row_absolute);
    assert!(matches!(
        parse_formula("A1:B2").unwrap(),
        Expr::Binary { .. }
    ));
}
#[test]
fn spans_require_legal_paired_axes_and_honor_grid_policy() {
    for (formula, first, last) in [
        ("Sheet1:Sheet3!A:C", (0, 0), (1048575, 2)),
        ("Sheet1:Sheet3!1:3", (0, 0), (2, 16383)),
    ] {
        let Expr::SheetSpan(span) = parse_formula(formula).unwrap() else {
            panic!("expected span: {formula}")
        };
        assert_eq!(span.first_sheet, "Sheet1");
        assert_eq!(span.last_sheet, "Sheet3");
        assert_eq!((span.start.row, span.start.column), first);
        let end = span.end.as_ref().unwrap();
        assert_eq!((end.row, end.column), last);
        assert_eq!(span.start.sheet, None);
        assert_eq!(end.sheet, None);
        let printed = FormulaPrinter::print(&Expr::SheetSpan(span.clone())).unwrap();
        assert_eq!(parse_formula(&printed).unwrap(), Expr::SheetSpan(span));
    }
    for formula in [
        "Sheet1:Sheet3!A",
        "Sheet1:Sheet3!1",
        "Sheet1:Sheet3!A:3",
        "Sheet1:Sheet3!A:XFE",
        "Sheet1:Sheet3!1:1048577",
        "Sheet1:Sheet3!A1:XFE1",
        "Sheet1:Sheet3!A0",
        "Sheet1:Sheet3!A1:Sheet3!B2",
        "'[book]Sheet1:Sheet3'!A1",
    ] {
        assert!(parse_formula(formula).is_err(), "accepted {formula}");
    }
    let policy = FormulaParsePolicy {
        max_rows: 2,
        max_columns: 2,
        ..Default::default()
    };
    assert!(parse_formula_with_policy("Sheet1:Sheet3!A1:B2", policy).is_ok());
    assert!(parse_formula_with_policy("Sheet1:Sheet3!A1:B3", policy).is_err());
    assert!(parse_formula_with_policy("Sheet1:Sheet3!C1", policy).is_err());
    assert!(parse_formula_with_policy("Sheet1:Sheet3!A:C", policy).is_err());
    assert!(parse_formula_with_policy("Sheet1:Sheet3!1:3", policy).is_err());
}
