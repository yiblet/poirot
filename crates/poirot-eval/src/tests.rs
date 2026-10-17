use super::*;
fn book() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    w
}
fn eval(w: &Workbook, f: &str) -> CellValue {
    match crate::run_formula(
        w,
        0,
        "Z20".parse().unwrap(),
        f,
        Default::default(),
        Default::default(),
    ) {
        crate::EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("evaluation aborted: {outcome:?}"),
    }
}
#[test]
fn precedence_coercion_and_errors() {
    let w = book();
    for (f, n) in [
        ("2^3^2", 64.),
        ("-2^2", 4.),
        ("2^200%", 4.),
        ("SUM(TRUE,\"2\",3)", 6.),
        ("ROUND(-1.25,1)", -1.3),
        ("MOD(-3,2)", 1.),
    ] {
        assert_eq!(eval(&w, f), Number(n), "{f}")
    }
    assert_eq!(eval(&w, "1/0"), Error(Div0));
    assert_eq!(eval(&w, "#N/A+#DIV/0!"), Error(NA));
    assert_eq!(eval(&w, "UNKNOWN(1/0)"), Error(Name));
    assert_eq!(eval(&w, "\"3\"=3"), Boolean(false));
}
#[test]
fn decimal_rounding_matches_spreadsheet_modes_and_zero_branches() {
    let w = book();
    for (formula, expected) in [
        ("ROUND(0.7*3.5,1)", 2.5),
        ("ROUND(-0.7*3.5,1)", -2.5),
        ("ROUND(249.99999999999997,-2)", 300.0),
        ("ROUNDUP(0.1*3,1)", 0.3),
        ("ROUNDDOWN(0.3*3,1)", 0.9),
        ("TRUNC(0.3*3,1)", 0.9),
        ("TRUNC(-1.9)", -1.0),
        ("ROUND(1E300,300)", 1e300),
    ] {
        assert_eq!(eval(&w, formula), Number(expected), "{formula}");
    }
    assert_eq!(eval(&w, "ROUNDUP(1,-309)"), Error(Num));
    for formula in [
        "ROUND(-0.01,0)",
        "ROUNDUP(-0.0,0)",
        "ROUNDDOWN(-0.01,0)",
        "TRUNC(-0.01)",
    ] {
        let Number(value) = eval(&w, formula) else {
            panic!("expected a number: {formula}")
        };
        assert_eq!(value.to_bits(), 0.0f64.to_bits(), "{formula}");
        assert_eq!(eval(&w, &format!("{formula}=0")), Boolean(true));
        assert_eq!(
            eval(&w, &format!("IF({formula}=0,\"\",\"nonzero\")")),
            Text("".into())
        );
    }
}
#[test]
fn references_names_and_sparse_columns() {
    let mut w = book();
    let s = w.sheet_mut(0).unwrap();
    s.set_value("A1".parse().unwrap(), true).unwrap();
    s.set_value("A2".parse().unwrap(), "2").unwrap();
    s.set_value("A3".parse().unwrap(), 3.).unwrap();
    assert_eq!(eval(&w, "SUM(A1:A3)"), Number(3.));
    assert_eq!(eval(&w, "SUM(A1,A2,A3)"), Number(3.));
    assert_eq!(eval(&w, "SUM(A:A)"), Number(3.));
    assert_eq!(eval(&w, "COUNTBLANK(A:A)"), Number((MAX_ROWS - 3) as f64));
    assert_eq!(eval(&w, "SUM(A1:A3 A3:B4)"), Number(3.));
    assert_eq!(eval(&w, "SUM((A3,A3))"), Number(6.));
    assert_eq!(eval(&w, "A1:A2 B1:B2"), Error(Null));
    w.define_name("Rate", "2", None).unwrap();
    w.define_name("Rate", "3", Some(0)).unwrap();
    assert_eq!(eval(&w, "Rate*2"), Number(6.));
}
#[test]
fn lazy_branches_and_cycles() {
    let mut w = book();
    for (f, v) in [
        ("IF(FALSE,1/0,4)", Number(4.)),
        ("IFERROR(1/0,5)", Number(5.)),
        ("IFNA(#N/A,6)", Number(6.)),
        ("IFNA(1/0,6)", Error(Div0)),
        ("CHOOSE(2,1/0,7)", Number(7.)),
    ] {
        assert_eq!(eval(&w, f), v)
    }
    w.sheet_mut(0)
        .unwrap()
        .set_formula("A1".parse().unwrap(), "A2", Number(9.))
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_formula("A2".parse().unwrap(), "A1", Number(9.))
        .unwrap();
    assert_eq!(
        match crate::run_cell(
            &w,
            0,
            "A1".parse().unwrap(),
            Default::default(),
            Default::default()
        ) {
            crate::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        Error(Ref)
    );
    w.define_name("Loop", "Loop", None).unwrap();
    assert_eq!(eval(&w, "Loop"), Error(Ref));
}
#[test]
fn mutations_and_recalculation() {
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), 2.)
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_formula("B1".parse().unwrap(), "A1*3", Number(-1.))
        .unwrap();
    assert_eq!(
        match crate::run_cell(
            &w,
            0,
            "B1".parse().unwrap(),
            Default::default(),
            Default::default()
        ) {
            crate::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        Number(6.)
    );
    w.sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), 4.)
        .unwrap();
    assert_eq!(
        match crate::run_cell(
            &w,
            0,
            "B1".parse().unwrap(),
            Default::default(),
            Default::default()
        ) {
            crate::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        Number(12.)
    );
    match crate::run_recalculation(&mut w, Default::default(), Default::default()) {
        crate::EvaluationOutcome::Complete { .. } => (),
        outcome => panic!("evaluation aborted: {outcome:?}"),
    };
    assert_eq!(
        w.sheet(0)
            .unwrap()
            .get("B1".parse().unwrap())
            .unwrap()
            .value,
        Number(12.)
    );
}
#[test]
fn empty_reference_and_zero_argument_aggregates() {
    let w = book();
    assert_eq!(eval(&w, "A1"), Blank);
    for f in [
        "SUM()",
        "COUNT()",
        "COUNTA()",
        "MIN()",
        "MAX()",
        "PRODUCT()",
    ] {
        assert_eq!(eval(&w, f), Number(0.), "{f}");
    }
    assert_eq!(eval(&w, "ISBLANK(A1)"), Boolean(true));
    assert_eq!(eval(&w, "AVERAGE()"), Error(Div0));
    assert_eq!(eval(&w, "AND()"), Error(VALUE_ERROR));
}
#[test]
fn cross_sheet_errors_and_implicit_intersection() {
    let mut w = book();
    w.add_sheet("Other Data").unwrap();
    w.sheet_mut(1)
        .unwrap()
        .set_value("A1".parse().unwrap(), 8.)
        .unwrap();
    assert_eq!(eval(&w, "'Other Data'!A1+2"), Number(10.));
    assert_eq!(eval(&w, "MissingSheet!A1"), Error(Ref));
    assert_eq!(eval(&w, "'Other Data'!A1:Sheet1!A1"), Error(Ref));
    w.sheet_mut(0)
        .unwrap()
        .set_value("A20".parse().unwrap(), 7.)
        .unwrap();
    assert_eq!(eval(&w, "A1:A30"), Number(7.));
    assert_eq!(eval(&w, "A1:B30"), Error(VALUE_ERROR));
    assert_eq!(eval(&w, "SUM({1,#N/A})"), Error(NA));
    assert_eq!(eval(&w, "COUNT({1,#N/A})"), Number(1.));
    assert_eq!(eval(&w, "COUNTA({1,#N/A})"), Number(2.));
    assert_eq!(eval(&w, "AND({TRUE,#N/A})"), Error(NA));
}
#[test]
fn resource_and_function_contracts() {
    let mut w = book();
    for i in 1..=140 {
        let a = format!("A{i}").parse().unwrap();
        let f = format!("A{}", i + 1);
        w.sheet_mut(0).unwrap().set_formula(a, f, Blank).unwrap();
    }
    assert!(matches!(
        crate::run_cell(
            &w,
            0,
            "A1".parse().unwrap(),
            Default::default(),
            Default::default()
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
    assert_eq!(eval(&w, "INDEX(A:A,0)"), Blank);
    assert!(matches!(
        crate::run_formula(
            &w,
            0,
            "Z20".parse().unwrap(),
            "MATCH(1,A:A,0)",
            Default::default(),
            Default::default()
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
    assert_eq!(eval(&w, "REPT(\"x\",32768)"), Error(VALUE_ERROR));
    for f in ["IF(1)", "ABS(1,2)", "NOT()", "ROUND(1)", "INDEX(A1)"] {
        assert_eq!(eval(&w, f), Error(VALUE_ERROR), "{f}");
    }
    assert_eq!(eval(&w, "AND(FALSE,#N/A)"), Error(NA));
    assert_eq!(eval(&w, "OR(TRUE,#N/A)"), Error(NA));
}
#[test]
fn invalid_public_model_numbers_never_compare_equal() {
    let mut w = book();
    for n in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        w.sheet_mut(0)
            .unwrap()
            .set_value("A1".parse().unwrap(), n)
            .unwrap();
        assert_eq!(
            match crate::run_cell(
                &w,
                0,
                "A1".parse().unwrap(),
                Default::default(),
                Default::default()
            ) {
                crate::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            Error(Num)
        );
        for formula in ["A1=1", "A1=A1", "SUM(A1)", "A1+0", "MATCH(A1,{1,2},0)"] {
            assert_eq!(eval(&w, formula), Error(Num), "{formula}");
        }
    }
}
#[test]
fn text_function_uses_shared_formatter() {
    let w = book();
    for (formula, want) in [
        ("TEXT(1234.5,\"#,##0.00\")", "1,234.50"),
        ("TEXT(60,\"yyyy-mm-dd\")", "1900-02-29"),
        ("TEXT(\"12\",\"0.0\")", "12.0"),
        ("TEXT(\"hello\",\"0\")", "hello"),
        ("TEXT(TRUE,\"0\")", "TRUE"),
    ] {
        assert_eq!(eval(&w, formula), Text(want.into()), "{formula}");
    }
    assert_eq!(eval(&w, "TEXT(1,\"[DBNum1]0\")"), Error(VALUE_ERROR));
    assert_eq!(eval(&w, "TEXT(#N/A,\"0\")"), Error(NA));
}
#[test]
fn arrays_statistics_text_and_lookup() {
    let w = book();
    for (f, v) in [
        ("SUM({1,2;3,4})", Number(10.)),
        ("AVERAGE({1,2,3})", Number(2.)),
        ("VAR.P({1,2,3})", Number(2. / 3.)),
        ("MEDIAN(3,1,2,4)", Number(2.5)),
        ("LEN(\"a😀b\")", Number(3.)),
        ("MID(\"a😀b\",2,1)", Text("😀".into())),
        ("TRIM(\"  a   b  \")", Text("a b".into())),
        ("MATCH(\"b*\",{\"a\",\"Bee\",\"c\"},0)", Number(2.)),
        ("VLOOKUP(2,{1,10;2,20},2,FALSE)", Number(20.)),
        ("INDEX({1,2;3,4},2,2)", Number(4.)),
    ] {
        assert_eq!(eval(&w, f), v, "{f}")
    }
}

#[test]
fn lookup_blanks_preserve_identity_through_consumers_and_formula_cells() {
    let mut w = book();
    let sheet = w.sheet_mut(0).unwrap();
    for (address, value) in [
        ("A1", "blank"),
        ("B1", "zero"),
        ("C1", "text"),
        ("E1", "blank"),
        ("E2", "zero"),
        ("E3", "text"),
    ] {
        sheet.set_value(address.parse().unwrap(), value).unwrap();
    }
    for address in ["B2", "F2"] {
        sheet.set_value(address.parse().unwrap(), 0.).unwrap();
    }
    for address in ["C2", "F3"] {
        sheet.set_value(address.parse().unwrap(), "").unwrap();
    }
    sheet
        .set_formula(
            "H1".parse().unwrap(),
            "HLOOKUP(\"blank\",A1:C2,2,FALSE)",
            Blank,
        )
        .unwrap();
    for lookup in [
        "HLOOKUP(\"blank\",A1:C2,2,FALSE)",
        "VLOOKUP(\"blank\",E1:F3,2,FALSE)",
        "LOOKUP(\"blank\",A1:A1,A2:A2)",
        "XLOOKUP(\"blank\",A1:C1,A2:C2)",
        "INDEX(A2:C2,1,1)",
    ] {
        for (formula, expected) in [
            (lookup.to_string(), Blank),
            (format!("MIN({lookup},0.03)"), Number(0.03)),
            (format!("MAX({lookup},-0.03)"), Number(-0.03)),
            (format!("ISBLANK({lookup})"), Boolean(true)),
            (format!("ISNUMBER({lookup})"), Boolean(false)),
            (format!("{lookup}+2"), Number(2.)),
            (format!("\"x\"&{lookup}"), Text("x".into())),
            (format!("MIN(IF(TRUE,{lookup},7),0.03)"), Number(0.03)),
            (
                format!("IF(ISBLANK({lookup}),\"empty\",\"filled\")"),
                Text("empty".into()),
            ),
        ] {
            assert_eq!(eval(&w, &formula), expected, "prepared: {formula}");
            let plan = bound::FormulaPlan::bind(&poirot_formula::parse_formula(&formula).unwrap())
                .unwrap();
            let tree =
                execute_configuration(&w, &Default::default(), Default::default(), |session| {
                    let at = "Z20".parse().unwrap();
                    let value = session.expr(0, at, &plan.expression())?;
                    session.result_scalar(0, at, value)
                });
            match tree {
                EvaluationOutcome::Complete { value, .. } => {
                    assert_eq!(value, expected, "tree: {formula}")
                }
                outcome => panic!("{formula}: {outcome:?}"),
            }
            match crate::run_plan(
                &w,
                0,
                "Z20".parse().unwrap(),
                &plan,
                Default::default(),
                Default::default(),
            ) {
                EvaluationOutcome::Complete { value, .. } => {
                    assert_eq!(value, expected, "bound: {formula}")
                }
                outcome => panic!("{formula}: {outcome:?}"),
            }
        }
    }
    for lookup in [
        "HLOOKUP(\"zero\",A1:C2,2,FALSE)",
        "VLOOKUP(\"zero\",E1:F3,2,FALSE)",
    ] {
        assert_eq!(eval(&w, &format!("ISBLANK({lookup})")), Boolean(false));
        assert_eq!(eval(&w, &format!("MIN({lookup},0.03)")), Number(0.));
    }
    for lookup in [
        "HLOOKUP(\"text\",A1:C2,2,FALSE)",
        "VLOOKUP(\"text\",E1:F3,2,FALSE)",
    ] {
        assert_eq!(eval(&w, lookup), Text("".into()));
        assert_eq!(eval(&w, &format!("ISBLANK({lookup})")), Boolean(false));
    }
    for lookup in [
        "HLOOKUP(\"absent\",A1:C2,2,FALSE)",
        "VLOOKUP(\"absent\",E1:F3,2,FALSE)",
    ] {
        assert_eq!(eval(&w, lookup), Error(NA));
    }
    // Formula publication and subsequent references retain the selected blank.
    assert_eq!(eval(&w, "ISBLANK(H1)"), Boolean(true));
    assert_eq!(eval(&w, "MIN(H1,0.03)"), Number(0.03));
}

#[test]
fn selection_preserves_types_until_the_selected_consumer() {
    let mut w = book();
    let sheet = w.sheet_mut(0).unwrap();
    sheet.set_value("A2".parse().unwrap(), true).unwrap();
    sheet.set_value("A3".parse().unwrap(), "12").unwrap();
    sheet.set_value("A4".parse().unwrap(), "").unwrap();
    sheet.set_value("A5".parse().unwrap(), Error(NA)).unwrap();
    // A1 is a real blank. Each selector must forward its selected value/reference
    // before the outer predicate, aggregate, arithmetic, or text consumer runs.
    for selected in [
        "IF(TRUE,{value},1/0)",
        "IFERROR({value},99)",
        "IFNA({value},99)",
        "CHOOSE(1,{value},1/0)",
        "IFS(TRUE,{value},FALSE,1/0)",
        "SWITCH(1,1,{value},2,1/0)",
    ] {
        for (template, expected) in [
            ("ISBLANK({selected})", Boolean(true)),
            ("MIN({selected},0.03)", Number(0.03)),
            ("{selected}+2", Number(2.)),
            ("\"x\"&{selected}", Text("x".into())),
        ] {
            let formula = template.replace("{selected}", &selected.replace("{value}", "A1"));
            assert_eq!(eval(&w, &formula), expected, "{formula}");
        }
        for (cell, predicate, expected) in [
            ("A2", "ISLOGICAL", true),
            ("A2", "ISNUMBER", false),
            ("A3", "ISTEXT", true),
            ("A3", "ISNUMBER", false),
            ("A4", "ISTEXT", true),
            ("A4", "ISBLANK", false),
        ] {
            let formula = format!("{predicate}({})", selected.replace("{value}", cell));
            assert_eq!(eval(&w, &formula), Boolean(expected), "{formula}");
        }
        let numeric_text = selected.replace("{value}", "A3");
        assert_eq!(eval(&w, &format!("{numeric_text}+1")), Number(13.));
        assert_eq!(eval(&w, &format!("SUM({numeric_text})")), Number(0.));
    }
    // Error handlers consume only their designated error kinds, not other values.
    for (formula, expected) in [
        ("ISNA(CHOOSE(1,A5,7))", Boolean(true)),
        ("IFERROR(A5,7)", Number(7.)),
        ("IFNA(A5,7)", Number(7.)),
        ("IFNA(1/0,7)", Error(Div0)),
        // Explicitly omitted selected results remain blank until consumed.
        ("ISBLANK(IF(TRUE,,7))", Boolean(true)),
        ("ISNUMBER(IF(TRUE,,7))", Boolean(false)),
        ("IF(FALSE,7)", Boolean(false)),
        // Explicit conversion functions intentionally replace types.
        ("N(A2)", Number(1.)),
        ("N(A3)", Number(0.)),
        ("T(A2)", Text("".into())),
        ("T(A3)", Text("12".into())),
        ("ISNUMBER(INDEX({\"12\",TRUE},1,1))", Boolean(false)),
        ("ISLOGICAL(INDEX({\"12\",TRUE},1,2))", Boolean(true)),
    ] {
        assert_eq!(eval(&w, formula), expected, "{formula}");
    }
}

#[test]
fn blank_argument_rules_distinguish_inline_values_from_selected_cells() {
    let mut workbook = book();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), "key")
        .unwrap();
    for consumer in ["AVERAGE", "AVERAGEA", "MEDIAN", "MIN", "MINA"] {
        assert_eq!(
            eval(&workbook, &format!("{consumer}(IF(TRUE,),1)")),
            Number(if matches!(consumer, "MIN" | "MINA") {
                0.
            } else {
                0.5
            }),
            "{consumer}"
        );
        assert_eq!(
            eval(&workbook, &format!("{consumer}(INDEX(A2:A2,1),1)")),
            Number(1.),
            "{consumer}"
        );
    }
    for selected in [
        "HLOOKUP(\"key\",A1:A2,2,FALSE)",
        "LOOKUP(\"key\",A1:A1,A2:A2)",
    ] {
        assert_eq!(eval(&workbook, selected), Blank);
        assert_eq!(
            eval(&workbook, &format!("ISREF({selected})")),
            Boolean(false)
        );
        assert_eq!(eval(&workbook, &format!("MIN({selected},1)")), Number(1.));
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value("A2".parse().unwrap(), true)
            .unwrap();
        assert_eq!(eval(&workbook, &format!("SUM({selected})")), Number(1.));
        assert_eq!(eval(&workbook, &format!("COUNT({selected})")), Number(1.));
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value("A2".parse().unwrap(), "abc")
            .unwrap();
        assert_eq!(
            eval(&workbook, &format!("SUM({selected})")),
            Error(VALUE_ERROR)
        );
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value("A2".parse().unwrap(), Blank)
            .unwrap();
    }
}
