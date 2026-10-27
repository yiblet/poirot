//! Actual dispatch regressions for DStarRunner coercion and short circuit branches.
use poirot::eval::compile::bound::FormulaPlan;
use poirot::model::{CellAddress, CellError as E, CellValue as V, Workbook};

fn check(formula: &str, expected: V) {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    let p = FormulaPlan::bind(&poirot_parsing::formula::parse_formula(formula).unwrap()).unwrap();
    assert_eq!(
        match poirot::eval::run_plan(
            &w,
            0,
            CellAddress { row: 0, col: 10 },
            &p,
            Default::default(),
            Default::default()
        ) {
            poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        expected,
        "{formula}"
    );
}

#[test]
fn field_rounding_near_half_and_criteria_int_bounds_use_source_rules() {
    check(
        r#"DSUM({"N";7},0.49999999999999994,{"N";TRUE})"#,
        V::Error(E::Value),
    );
    check(
        r#"DCOUNT({"N";7},0.49999999999999994,{"N";TRUE})"#,
        0.0.into(),
    );
    // Typed TRUE filters for numeric one; numeric seven selects the data row.
    check(r#"DCOUNT({"N";7},0.49999999999999994,{"N";7})"#, 1.0.into());
    check(
        r#"DSUM({"N","M";7,11},1.4999999999999998,{"N";7})"#,
        7.0.into(),
    );
    check(r#"DSUM({"N","M";7,11},1.5,{"N";7})"#, 11.0.into());
    check(r#"DSUM({"N","M";7,11},2,{1.9;7})"#, 11.0.into());
    for heading in ["2147483648", "-2147483649"] {
        check(
            &format!(r#"DSUM({{"N";7}},1,{{{heading};7}})"#),
            V::Error(E::Value),
        );
        check(
            &format!(r#"IFERROR(DSUM({{"N";7}},1,{{{heading};7}}),99)"#),
            99.0.into(),
        );
    }
    // First false AND operand bypasses the overflowing numeric heading.
    check(r#"DSUM({"N";7},1,{"N",2147483648;99,7})"#, 0.0.into());
    check(r#"DSUM({"N";7},1,{TRUE;7})"#, 7.0.into());
}

#[test]
fn duplicate_headers_numeric_text_error_and_statistic_collection() {
    check(r#"DSUM({"n","N";3,99},"N",{"n";3})"#, 3.0.into());
    check(r#"DSUM({42,TRUE;5,7},"42",{"TRUE";7})"#, 5.0.into());
    check(
        r#"DCOUNTA({"N";12;"12";" 0x1.8p3D "},1,{"N";12})"#,
        3.0.into(),
    );
    check(
        r#"DCOUNTA({"N";12;"12";" 0x1.8p3D "},1,{"N";"=12"})"#,
        1.0.into(),
    );
    check(r#"DGET({"N";#N/A;#DIV/0!},1,{"N";#N/A})"#, V::Error(E::NA));
    check(r#"DCOUNTA({"N";#N/A;#DIV/0!},1,{"N";#N/A})"#, 1.0.into());
    check(
        r#"DVAR({"N";2;TRUE;FALSE;"100";#N/A},1,{"N";">=-1"})"#,
        1.0.into(),
    );
    check(
        r#"DVARP({"N";2;TRUE;FALSE;"100";#N/A},1,{"N";">=-1"})"#,
        (2. / 3.).into(),
    );
    check(r#"DSTDEVP({"N";7;"100"},1,{"N";">=-1"})"#, 0.0.into());
    check(r#"DSTDEV({"N";7;"100"},1,{"N";">=-1"})"#, V::Error(E::Div0));
}

#[test]
fn blank_conditions_and_get_termination_preserve_laziness() {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    for (address, value) in [
        ("A1", "N".into()),
        ("A2", 1.0.into()),
        ("A3", 1.0.into()),
        ("A4", 9.0.into()),
        ("C1", "N".into()),
        ("D1", "Unknown".into()),
        ("C2", 1.0.into()),
        ("D2", V::Blank),
        ("C3", 9.0.into()),
        ("D3", true.into()),
        ("F1", 2147483648.0.into()),
        ("F2", V::Blank),
    ] {
        w.sheet_mut(0).unwrap().set_value(address, value).unwrap();
    }
    for (formula, expected) in [
        ("DGET(A1:A4,1,C1:D3)", V::Error(E::Num)),
        ("DSUM(A1:A4,1,F1:F2)", 11.0.into()),
    ] {
        let p =
            FormulaPlan::bind(&poirot_parsing::formula::parse_formula(formula).unwrap()).unwrap();
        assert_eq!(
            match poirot::eval::run_plan(
                &w,
                0,
                CellAddress { row: 0, col: 10 },
                &p,
                Default::default(),
                Default::default()
            ) {
                poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{formula}"
        );
    }
}
