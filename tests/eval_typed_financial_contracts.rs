//! Public financial contracts retained by declarative numeric and collection execution.
use poirot::eval::{EvaluationOutcome, run_formula};
use poirot::model::{CellAddress, CellError, CellValue, Workbook};

fn calculate(formula: &str) -> CellValue {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    match run_formula(
        &workbook,
        0,
        CellAddress { row: 0, col: 0 },
        formula,
        Default::default(),
        Default::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("{outcome:?}"),
    }
}

#[test]
fn loan_defaults_and_decimal_admission_survive() {
    // Zero-rate interest is zero even with an explicitly omitted rate.
    assert_eq!(calculate("IPMT(,1,12,120)"), CellValue::Number(0.0));
    assert_eq!(
        calculate("DOLLARDE(TRUE,16)"),
        CellValue::Error(CellError::Value)
    );
}

#[test]
fn whole_collections_keep_schedule_blanks_and_cashflow_origins() {
    // Schedule blanks denote a zero rate; array text is excluded from NPV periods.
    assert_eq!(calculate("FVSCHEDULE(100,A2:A4)"), CellValue::Number(100.0));
    assert_eq!(calculate("NPV(0,{TRUE,\"2\",3})"), CellValue::Number(3.0));
    assert_eq!(calculate("NPV(0,TRUE,\"2\")"), CellValue::Number(3.0));
    let CellValue::Number(value) = calculate("XNPV(0,{-100;120},{1;366})") else {
        panic!("numeric discounted cash flows")
    };
    assert!((value - 20.0).abs() < 1e-12);
    assert_eq!(
        calculate("XNPV(0,{\"bad\";1},{1/0;2})"),
        CellValue::Error(CellError::Value)
    );
}

#[test]
fn child_first_numeric_text_and_collection_controls_spill_in_both_evaluators() {
    use poirot::eval::overlay::{Evaluation, Overlay};
    use poirot::eval::{EvaluationContext, EvaluationTarget};
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    for (cell, formula) in [
        ("A1", "FV({0;0},10,{-20,-30})"),
        ("D1", "NPV({0;0},{-100;120})"),
        ("G1", "EUROCONVERT({1;2},\"EUR\",\"EUR\")"),
        ("J1", "FIXED({1;2},2,TRUE)"),
        ("M1", "IRR({-100;121},{0.1;0.2})"),
        ("P1", "MIRR({-100;0;121},{0;0.2},{0,0.3})"),
        ("T1", "XIRR({-100;121},{1;366},{0.1;0.2})"),
        ("W1", "FVSCHEDULE({100;200},{0.1;0.2})"),
        ("Z1", "XNPV({0;0},{-100;120},{1;366})"),
    ] {
        sheet.set_formula(cell, formula).unwrap();
    }
    let target = |cell: &str| EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    };
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&workbook)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    // Children are requested before every origin. Expected values follow zero-rate
    // annuities, cash-flow equations, and same-currency identity conversion.
    for (cell, expected) in [
        ("B2", 300.0),
        ("D2", 20.0),
        ("G2", 2.0),
        ("M2", 0.21),
        ("Q2", 0.1),
        ("T2", 0.21),
        ("W2", 264.0),
        ("Z2", 20.0),
    ] {
        for actual in [
            direct.cell(target(cell)).unwrap(),
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target(cell))
                .unwrap(),
        ] {
            let CellValue::Number(actual) = actual else {
                panic!("{cell}: {actual:?}")
            };
            assert!(
                (actual - expected).abs() < 1e-8,
                "{cell}: {actual} != {expected}"
            );
        }
    }
    for actual in [
        direct.cell(target("J2")).unwrap(),
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("J2"))
            .unwrap(),
    ] {
        assert_eq!(actual, CellValue::Text("2.00".into()));
    }
}

#[test]
fn all_financial_catalog_shapes_match_their_typed_source_roles() {
    use poirot::eval::functions::registry::{FunctionId, ResultRule};
    let mut count = 0;
    for descriptor in poirot::eval::functions::registry::FUNCTION_DESCRIPTORS.iter() {
        if !matches!(descriptor.handler, FunctionId::Financial(_)) {
            continue;
        }
        count += 1;
        let expected = match descriptor.name {
            "DOLLAR" | "FIXED" => ResultRule::ElementwiseText,
            "NPV" | "FVSCHEDULE" | "XNPV" => ResultRule::FirstNumericArgument,
            "IRR" => ResultRule::SecondNumericArgument,
            "MIRR" => ResultRule::AfterFirstNumericArguments,
            "XIRR" => ResultRule::ThirdNumericArgument,
            _ => ResultRule::ElementwiseNumber,
        };
        assert_eq!(descriptor.result, expected, "{}", descriptor.name);
    }
    assert_eq!(count, 58);
}

#[test]
fn modern_financial_arity_rejects_outside_documented_signatures() {
    // Microsoft syntax: RATE(nper,pmt,pv,[fv],[type],[guess]), MIRR(values,
    // finance_rate,reinvest_rate), and IPMT/PPMT(rate,per,nper,pv,[fv],[type]).
    // https://support.microsoft.com/en-us/excel/functions/rate-function
    // https://support.microsoft.com/en-us/excel/functions/mirr-function
    // https://support.microsoft.com/en-us/excel/functions/ipmt-function
    // https://support.microsoft.com/en-us/excel/functions/ppmt-function
    // NPV permits one rate and 1..254 cash-flow arguments:
    // https://support.microsoft.com/en-us/excel/functions/npv-function
    for formula in [
        "RATE(1,2,3,4,5,6,7)",
        "MIRR()",
        "MIRR({-100;121},0)",
        "MIRR({-100;121},0,0,0)",
        "IPMT(0,1,12,120,0,0,1)",
        "PPMT(0,1,12,120,0,0,1)",
    ] {
        assert_eq!(
            calculate(formula),
            CellValue::Error(CellError::Value),
            "{formula}"
        );
    }
    let npv = poirot::eval::FUNCTION_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.name == "NPV")
        .unwrap();
    assert!(npv.arity.accepts(2));
    assert!(npv.arity.accepts(255));
    assert!(!npv.arity.accepts(256));
    assert_eq!(calculate("IPMT(0,1,12,120,0,0)"), CellValue::Number(0.0));
    assert_eq!(calculate("PPMT(0,1,12,120,0,0)"), CellValue::Number(-10.0));
}

#[test]
fn mirr_keeps_zero_periods_and_rejects_samples_without_both_signs() {
    // Microsoft includes zero periods; a terminal receipt is not reinvested for an extra year.
    // https://support.microsoft.com/en-us/excel/functions/mirr-function
    for formula in [
        "MIRR({-100;0;121},0.2,0.3)",
        "MIRR({-100;0;0;133.1},0.2,0.3)",
    ] {
        let CellValue::Number(value) = calculate(formula) else {
            panic!("{formula}")
        };
        assert!((value - 0.1).abs() < 1e-12, "{formula}: {value}");
    }
    for formula in [
        "MIRR({0;0},0.1,0.1)",
        "MIRR({-10;-20},0.1,0.1)",
        "MIRR({10;20},0.1,0.1)",
    ] {
        assert_eq!(
            calculate(formula),
            CellValue::Error(CellError::Div0),
            "{formula}"
        );
    }
}
