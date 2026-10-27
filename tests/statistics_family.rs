use poirot::eval::{
    self as eval, EvaluationBudgets, EvaluationError, EvaluationInputs, EvaluationOutcome,
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
        ("PERCENTRANK.INC({1,2,3},2,3)", 0.5),
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
                run_work: 0,
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
        let plan = eval::compile::bound::FormulaPlan::bind(
            &poirot_parsing::formula::parse_formula(formula).unwrap(),
        )
        .unwrap();
        assert!(
            matches!(
                eval::run_plan(&book, 0, AT, &plan, EvaluationInputs::default(), budgets),
                EvaluationOutcome::Aborted {
                    failure: EvaluationError::Budget(_),
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
        .set_formula_with_cached_value(
            CellAddress { row: 0, col: 0 },
            "1/0",
            CellValue::Number(99.),
        )
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value(
            CellAddress { row: 1, col: 0 },
            "NORMSDIST(0)",
            CellValue::Number(99.),
        )
        .unwrap();
    let plan = eval::compile::bound::FormulaPlan::bind(
        &poirot_parsing::formula::parse_formula("AVERAGEA(A1:A2)").unwrap(),
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
fn statistical_source_collection_and_second_argument_precedence() {
    let book = workbook();
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
    // Excel AVERAGEA accepts up to 255 arguments; every admitted value is one.
    // https://support.microsoft.com/en-us/excel/functions/averagea-function
    for count in [31, 255] {
        let arguments = vec!["1"; count].join(",");
        assert_eq!(
            result(&book, &format!("AVERAGEA({arguments})")),
            CellValue::Number(1.)
        );
    }
    let too_many = vec!["1"; 256].join(",");
    assert_eq!(
        result(&book, &format!("AVERAGEA({too_many})")),
        CellValue::Error(CellError::Value)
    );
}

#[test]
fn statistical_domain_and_sample_shape_errors_remain_cells() {
    let book = workbook();
    for (formula, error) in [
        ("AVEDEV()", CellError::Value),
        ("LARGE({1,2},0)", CellError::Num),
        ("PERCENTILE({1,2},2)", CellError::Num),
        ("PERCENTRANK({1,2},1,0)", CellError::Num),
        ("CORREL({1,2},{1})", CellError::NA),
    ] {
        assert_eq!(result(&book, formula), CellValue::Error(error), "{formula}");
    }
    // Pairing follows flattened position, so squared differences are 1+4+9.
    assert_eq!(
        result(&book, "SUMXMY2({1,2,3},{2;4;6})"),
        CellValue::Number(14.)
    );
}
