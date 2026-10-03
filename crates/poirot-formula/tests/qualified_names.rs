use poirot_formula::{BinaryOp, Expr, FormulaPrinter, parse_formula};
#[test]
fn qualified_names_preserve_scope_and_apostrophes() {
    for (source, sheet, name) in [
        ("Sheet2!LocalName", "Sheet2", "LocalName"),
        ("'Sheet 2'!Rate", "Sheet 2", "Rate"),
        ("'O''Brien'!ABC", "O'Brien", "ABC"),
    ] {
        let expected = Expr::QualifiedName {
            sheet: sheet.into(),
            name: name.into(),
        };
        assert_eq!(parse_formula(source).unwrap(), expected);
        assert_eq!(
            parse_formula(&FormulaPrinter::print(&expected).unwrap()).unwrap(),
            expected
        );
    }
}
#[test]
fn qualification_preserves_strings_cells_and_column_ranges() {
    assert_eq!(
        parse_formula("\"Sheet2!Rate\"").unwrap(),
        Expr::String("Sheet2!Rate".into())
    );
    assert!(matches!(
        parse_formula("'Sheet 2'!A1").unwrap(),
        Expr::Cell(_)
    ));
    let range = parse_formula("'Sheet 2'!A:C").unwrap();
    assert!(
        matches!(&range, Expr::Binary { op: BinaryOp::Range, left, right }
        if matches!(left.as_ref(), Expr::Column { column: 0, sheet: Some(s), .. } if s == "Sheet 2")
        && matches!(right.as_ref(), Expr::Column { column: 2, sheet: Some(s), .. } if s == "Sheet 2"))
    );
    assert_eq!(
        parse_formula(&FormulaPrinter::print(&range).unwrap()).unwrap(),
        range
    );
    assert!(parse_formula("'Sheet 2'!Rate!").is_err());
    assert!(parse_formula("'Sheet 2'!Rate()").is_err());
}
