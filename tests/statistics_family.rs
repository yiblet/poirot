use poirot::eval::{
    self, EvaluationBudgets, EvaluationFailure, EvaluationInputs, EvaluationOutcome,
};
use poirot::model::{CellAddress, CellError, CellValue, Workbook};

const AT: CellAddress = CellAddress { row: 9, col: 9 };
fn workbook() -> Workbook {
    let mut book = Workbook::new();
    book.add_sheet("Sheet1").unwrap();
    for (address, value) in [
        (CellAddress { row: 0, col: 0 }, CellValue::Number(1.)),
        (CellAddress { row: 1, col: 0 }, CellValue::Text("x".into())),
        (CellAddress { row: 2, col: 0 }, CellValue::Number(3.)),
        (CellAddress { row: 0, col: 1 }, CellValue::Number(2.)),
        (CellAddress { row: 1, col: 1 }, CellValue::Number(999.)),
        (CellAddress { row: 2, col: 1 }, CellValue::Number(6.)),
    ] {
        book.sheet_mut(0)
            .unwrap()
            .set_value(address, value)
            .unwrap();
    }
    book
}
fn result(book: &Workbook, formula: &str) -> CellValue {
    match eval::run_formula(book, 0, AT, formula, Default::default(), Default::default()) {
        eval::EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("evaluation aborted: {outcome:?}"),
    }
}
#[test]
fn actual_formulas_cover_aggregates_order_statistics_and_aliases() {
    let book = workbook();
    for (formula, expected) in [
        ("AVEDEV(1,2,3)", 2. / 3.),
        ("AVERAGEA(2,TRUE,\"x\")", 1.),
        ("DEVSQ(1,2,3)", 2.),
        ("GEOMEAN(1,4)", 2.),
        ("MAXA(1,TRUE,\"2\")", 2.),
        ("MINA(1,TRUE,\"2\")", 1.),
        ("STDEVA(1,3)", 2_f64.sqrt()),
        ("STDEVPA(1,3)", 1.),
        ("VARA(1,3)", 2.),
        ("VARPA(1,3)", 1.),
        ("MODE({1,2,2})", 2.),
        ("LARGE({1,3,2},2)", 2.),
        ("SMALL({1,3,2},2)", 2.),
        ("PERCENTILE({1,2,3},0.5)", 2.),
        ("PERCENTILE.INC({1,2,3},0.5)", 2.),
        ("RANK(3,A1:A3)", 1.),
        ("RANK(1,{1,2})", 2.),
        ("PERCENTRANK({1,2,3},2)", 0.5),
        ("PERCENTRANK.INC({1,2,3},2,3,UNKNOWN_FUNCTION())", 0.5),
        ("PERCENTRANK.EXC({1,2,3},2)", 0.5),
        ("SUMXMY2(A1:A3,B1:B3)", 10.),
        ("SUMX2MY2({1,3},{2,6})", -30.),
        ("SUMX2PY2({1,3},{2,6})", 50.),
        ("CORREL({1,2,3},{2,4,6})", 1.),
        ("PEARSON({1,2,3},{2,4,6})", 1.),
        ("COVAR({1,2,3},{2,4,6})", 4. / 3.),
        ("COVARIANCE.P({1,2,3},{2,4,6})", 4. / 3.),
        ("COVARIANCE.S({1,2,3},{2,4,6})", 2.),
        ("SLOPE({2,4,6},{1,2,3})", 2.),
        ("INTERCEPT({2,4,6},{1,2,3})", 0.),
        ("FORECAST(4,{2,4,6},{1,2,3})", 8.),
        ("FORECAST.LINEAR(4,{2,4,6},{1,2,3})", 8.),
        ("NORMDIST(0,0,1,TRUE)", 0.5),
        ("NORM.DIST(0,0,1,TRUE)", 0.5),
        ("NORMINV(0.5,0,1)", 0.),
        ("NORM.INV(0.5,0,1)", 0.),
        ("NORMSINV(0.5)", 0.),
        ("NORM.S.INV(0.5)", 0.),
        ("NORMSDIST(0)", 0.5),
        ("NORM.S.DIST(0,TRUE)", 0.5),
        ("STANDARDIZE(3,1,2)", 1.),
        ("POISSON(0,0,UNKNOWN_FUNCTION())", 1.),
        ("POISSON.DIST(0,0,UNKNOWN_FUNCTION())", 1.),
        ("TDIST(0,10,2)", 1.),
        ("T.DIST(0,10,TRUE)", 0.5),
        ("T.DIST.RT(0,10)", 0.5),
        ("T.DIST.2T(0,10)", 1.),
    ] {
        let CellValue::Number(actual) = result(&book, formula) else {
            panic!("{formula} did not return a number")
        };
        assert!(
            (actual - expected).abs() < 1e-12,
            "{formula}: {actual} != {expected}"
        );
    }
}
#[test]
fn pinned_errors_arity_domain_and_operand_order_are_formula_visible() {
    let book = workbook();
    for (formula, expected) in [
        ("AVEDEV()", CellError::Value),
        ("NORMSDIST(0,UNKNOWN_FUNCTION())", CellError::Value),
        ("LARGE({1,2},0)", CellError::Num),
        ("SMALL({1,2},1/0)", CellError::Value),
        ("PERCENTILE({1,2},2)", CellError::Num),
        ("RANK(1,1/0,-0.1)", CellError::Num),
        ("PERCENTRANK(1/0,NA())", CellError::Num),
        ("PERCENTRANK({1,2},1,0)", CellError::Num),
        ("CORREL({1,2},{1})", CellError::NA),
        ("SUMXMY2({#DIV/0!,2},{#N/A})", CellError::NA),
        ("NORMDIST(0,0,-1,UNKNOWN_FUNCTION())", CellError::Num),
        ("NORMSINV(TRUE)", CellError::Value),
        ("TDIST(-1,UNKNOWN_FUNCTION(),2)", CellError::Num),
    ] {
        assert_eq!(
            match eval::run_formula_with_configuration(
                &book,
                0,
                AT,
                formula,
                &eval::RunConfiguration::default()
                    .with_builtin_policy(eval::BuiltinSemanticsPolicy::PinnedPoi),
                Default::default()
            ) {
                EvaluationOutcome::Complete { value, .. } => value,
                other => panic!("{other:?}"),
            },
            CellValue::Error(expected),
            "{formula}"
        );
    }
}
#[test]
fn operational_limits_cannot_be_caught_by_iferror() {
    let book = workbook();
    for (formula, budgets) in [
        (
            "IFERROR(LARGE(A1:A100,1),42)",
            EvaluationBudgets {
                materialized_values: 10,
                ..EvaluationBudgets::default()
            },
        ),
        (
            "IFERROR(NORMSDIST(0),42)",
            EvaluationBudgets {
                run_work: 20,
                ..EvaluationBudgets::default()
            },
        ),
        (
            "IFERROR(AVERAGEA(\"long text\"),42)",
            EvaluationBudgets {
                text_bytes: 2,
                ..EvaluationBudgets::default()
            },
        ),
    ] {
        let plan = eval::bound::FormulaPlan::bind(
            &poirot::parsing::formula::parse_formula(formula).unwrap(),
        )
        .unwrap();
        assert!(
            matches!(
                eval::run_plan(&book, 0, AT, &plan, EvaluationInputs::default(), budgets),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::Budget(_),
                    ..
                }
            ),
            "{formula}"
        );
    }
}

#[test]
fn indexed_reference_traversal_stops_before_later_operational_failure() {
    let mut book = workbook();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            CellAddress { row: 0, col: 0 },
            "1/0",
            CellValue::Number(99.),
        )
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            CellAddress { row: 1, col: 0 },
            "NORMSDIST(0)",
            CellValue::Number(99.),
        )
        .unwrap();
    let plan = eval::bound::FormulaPlan::bind(
        &poirot::parsing::formula::parse_formula("AVERAGEA(A1:A2)").unwrap(),
    )
    .unwrap();
    let outcome = eval::run_plan(
        &book,
        0,
        AT,
        &plan,
        EvaluationInputs::default(),
        EvaluationBudgets {
            run_work: 100,
            ..EvaluationBudgets::default()
        },
    );
    assert!(matches!(
        outcome,
        EvaluationOutcome::Complete {
            value: CellValue::Error(CellError::Div0),
            ..
        }
    ));
    assert_eq!(outcome.usage().cell_reads, 1);
}

#[test]
fn unsupported_adapter_topology_stays_operational_through_iferror() {
    let book = workbook();
    for formula in [
        "IFERROR(LARGE({1,2},),42)",
        "IFERROR(NORMDIST(0,0,1,A1),42)",
    ] {
        let plan = eval::bound::FormulaPlan::bind(
            &poirot::parsing::formula::parse_formula(formula).unwrap(),
        )
        .unwrap();
        assert!(
            matches!(
                eval::run_plan(
                    &book,
                    0,
                    AT,
                    &plan,
                    EvaluationInputs::default(),
                    EvaluationBudgets::default()
                ),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::UnsupportedCapability { .. },
                    ..
                }
            ),
            "{formula}"
        );
    }
}

#[test]
fn statistical_source_collection_and_second_argument_precedence() {
    let book = workbook();
    for formula in ["LARGE(1/0,NA())", "SMALL(1/0,NA())", "PERCENTILE(1/0,NA())"] {
        assert_eq!(
            result(&book, formula),
            CellValue::Error(CellError::Value),
            "{formula}"
        );
    }
    for formula in [
        "LARGE(1/0,0)",
        "SMALL(1/0,0)",
        "PERCENTILE(1/0,2)",
        "LARGE(1/0,2147483648)",
        "SMALL(1/0,2147483647.1)",
    ] {
        assert_eq!(
            result(&book, formula),
            CellValue::Error(CellError::Num),
            "{formula}"
        );
    }
    for formula in [
        "MAXA(1,\"x\")",
        "MINA(1,\"x\")",
        "AVEDEV(\"0x1p0\",2)",
        "GEOMEAN(\"1d\",2)",
        "CORREL({\"0x1p0\",2,3},{1,2,3})",
    ] {
        let expected = if formula.starts_with("CORREL") {
            CellValue::Number(1.)
        } else {
            CellValue::Error(CellError::Value)
        };
        assert_eq!(result(&book, formula), expected, "{formula}");
    }
    assert_eq!(result(&book, "MINA(A1:A3)"), CellValue::Number(0.));
    assert_eq!(result(&book, "MINA(A1:A4)"), CellValue::Number(0.));
    assert_eq!(
        result(&book, "MAXA({\"100\",TRUE,2})"),
        CellValue::Number(2.)
    );
    let thirty_one = vec!["1"; 31].join(",");
    assert_eq!(
        result(&book, &format!("AVERAGEA({thirty_one})")),
        CellValue::Number(1.)
    );
    let too_many = vec!["1"; 256].join(",");
    assert_eq!(
        result(&book, &format!("AVERAGEA({too_many})")),
        CellValue::Error(CellError::Value)
    );
}

#[test]
fn pair_topology_selects_shape_and_error_precedence() {
    let mut book = workbook();
    for (address, error) in [
        (CellAddress { row: 0, col: 2 }, CellError::NA),
        (CellAddress { row: 1, col: 2 }, CellError::Div0),
    ] {
        book.sheet_mut(0)
            .unwrap()
            .set_value(address, CellValue::Error(error))
            .unwrap();
    }
    for (formula, expected) in [
        ("SLOPE({#N/A,2},{1,#DIV/0!})", CellError::Div0),
        ("INTERCEPT({#N/A,2},{1,#DIV/0!})", CellError::Div0),
        ("SUMXMY2({#N/A,2},{1,#DIV/0!})", CellError::NA),
        ("SUMX2MY2({#DIV/0!,2},{1})", CellError::NA),
        ("SUMX2PY2({#DIV/0!,2},{1})", CellError::NA),
        ("SLOPE({#DIV/0!,2},{1})", CellError::NA),
        ("INTERCEPT({1,2},{#DIV/0!})", CellError::NA),
        ("CORREL({#DIV/0!,2},{1})", CellError::Div0),
        ("SUMXMY2(C1:C2,A1)", CellError::NA),
        ("SUMXMY2(NA(),{1,2})", CellError::NA),
        ("SUMXMY2(1/0,{1,2})", CellError::Div0),
        ("SLOPE(1/0,{#N/A,2})", CellError::Div0),
        ("INTERCEPT({#DIV/0!,2},NA())", CellError::NA),
    ] {
        assert_eq!(
            result(&book, formula),
            CellValue::Error(expected),
            "{formula}"
        );
    }
    // Equal element counts admit a row paired with a column.
    assert_eq!(
        result(&book, "SUMXMY2({1,2,3},{2;4;6})"),
        CellValue::Number(14.)
    );
}
