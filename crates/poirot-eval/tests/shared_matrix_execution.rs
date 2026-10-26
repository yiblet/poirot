//! Public matrix controls derived independently from arithmetic and documented array geometry.
//! Shape/default contracts: https://support.microsoft.com/en-us/excel/functions/sequence-function
//! https://support.microsoft.com/en-us/excel/functions/munit-function
//! https://support.microsoft.com/en-us/excel/functions/sumproduct-function
//! Other transforms use their named Microsoft function contracts, with explicit small permutations.
use poirot_eval::{
    BuiltinSemanticsPolicy, EvaluationBudgets, EvaluationFailure, EvaluationInputs,
    EvaluationOutcome, EvaluationTarget,
    overlay::{Evaluation, EvaluationContext, Overlay, OverlayPolicy},
    run_formula,
};
use poirot_model::{CellError, CellValue, ExcelText, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
fn close(value: CellValue, expected: f64, source: &str) {
    let CellValue::Number(value) = value else {
        panic!("{source}: {value:?}")
    };
    assert!(
        (value - expected).abs() < 1e-9,
        "{source}: {value} != {expected}"
    );
}
#[test]
fn every_matrix_handler_reaches_public_direct_and_overlay_execution() {
    // Regression inputs come from y=1+2*x or y=2^x; no Poirot result supplies an expectation.
    let cases = [
        ("MDETERM({1,2;3,4})", "D10", -2.),
        ("MINVERSE({1,2;3,4})", "E11", -0.5),
        ("MMULT({1,2;3,4},{1,0;0,1})", "E11", 4.),
        ("TRANSPOSE({1,2;3,4})", "E11", 4.),
        ("FREQUENCY({1,2,3,4},{2,3})", "D12", 1.),
        ("SUMPRODUCT({1,2;3,4},{2,2;2,2})", "D10", 20.),
        ("TREND({3;5;7},{1;2;3},{4;5})", "D11", 11.),
        ("GROWTH({2;4;8},{1;2;3},{4;5})", "D11", 32.),
        ("LINEST({3;5;7},{1;2;3})", "E10", 1.),
        ("LOGEST({2;4;8},{1;2;3})", "E10", 1.),
        ("MUNIT(2)", "E11", 1.),
        ("GROUPBY({\"b\";\"a\";\"b\"},{1;2;3},SUM,0,0)", "E11", 4.),
        ("PERCENTOF({1,2},{3,3})", "D10", 0.5),
        ("HSTACK({1;2},{3;4})", "E11", 4.),
        ("VSTACK({1,2},{3,4})", "E11", 4.),
        ("TAKE({1,2;3,4},1)", "E10", 2.),
        ("DROP({1,2;3,4},1)", "E10", 4.),
        ("CHOOSECOLS({1,2;3,4},2)", "D11", 4.),
        ("CHOOSEROWS({1,2;3,4},2)", "E10", 4.),
        ("EXPAND({1},2,2,9)", "E11", 9.),
        ("FILTER({1,2;3,4},{FALSE;TRUE})", "E10", 4.),
        ("SORT({3;1;2})", "D12", 3.),
        ("SORTBY({10;20;30},{3;1;2})", "D12", 10.),
        ("UNIQUE({1;1;2})", "D11", 2.),
        ("TOCOL({1,2;3,4})", "D13", 4.),
        ("TOROW({1,2;3,4})", "G10", 4.),
        ("WRAPCOLS({1,2,3,4},2)", "E11", 4.),
        ("WRAPROWS({1,2,3,4},2)", "E11", 4.),
        ("RANDARRAY(2,2,7,7.1,TRUE)", "E11", 7.),
        ("TRIMRANGE({0,1;0,2})", "E11", 2.),
        ("SEQUENCE(2,2,3,2)", "E11", 9.),
    ];
    assert_eq!(cases.len(), 31);
    for (formula, child, expected) in cases {
        let mut book = Workbook::new();
        book.add_sheet("S")
            .unwrap()
            .set_formula(at("D10").address, formula)
            .unwrap();
        let policy = OverlayPolicy {
            builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
            ..Default::default()
        };
        let context = EvaluationContext::default();
        let mut direct = Evaluation::with_workbook(&book)
            .with_policy(policy.clone())
            .build(context.clone())
            .unwrap();
        // Ask for the spill child before its anchor, exercising result-shape discovery.
        close(direct.cell(at(child)).unwrap(), expected, formula);
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(policy)
            .build()
            .unwrap();
        close(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at(child))
                .unwrap(),
            expected,
            formula,
        );
    }
}
#[test]
fn rearrangement_preserves_unpaired_text_units_and_filter_fallback_is_lazy() {
    let text = CellValue::TextUtf16(ExcelText::from_units(vec![0xd800, 0x61]));
    for formula in [
        "TRANSPOSE(A1:A2)",
        "HSTACK(A1,A2)",
        "CHOOSECOLS(A1:B1,1)",
        "FILTER(A1:A2,{TRUE;FALSE},1/0)",
    ] {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet.set_value(at("A1").address, text.clone()).unwrap();
        sheet.set_value(at("A2").address, 2.).unwrap();
        sheet.set_formula(at("D10").address, formula).unwrap();
        let mut evaluation = Evaluation::with_workbook(&book)
            .build(Default::default())
            .unwrap();
        assert_eq!(evaluation.cell(at("D10")).unwrap(), text, "{formula}");
    }
}
#[test]
fn matrix_failures_remain_cells_or_uncatchable_operational_failures() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    for (formula, error) in [
        ("MDETERM({1,#N/A;3,\"x\"})", CellError::Value),
        ("MINVERSE({1,2;2,4})", CellError::Num),
        ("MMULT({1,2},{3,4})", CellError::Value),
        ("SUMPRODUCT({1;2},{3,4})", CellError::Value),
        ("SEQUENCE(,)", CellError::Value),
        ("MUNIT(0)", CellError::Value),
    ] {
        assert!(
            matches!(run_formula(&book, 0, at("D10").address, formula, EvaluationInputs::default(), EvaluationBudgets::default()), EvaluationOutcome::Complete { value: CellValue::Error(actual), .. } if actual == error),
            "{formula}"
        );
    }
    assert!(matches!(
        run_formula(
            &book,
            0,
            at("D10").address,
            "IFERROR(SEQUENCE(3,3),99)",
            EvaluationInputs::default(),
            EvaluationBudgets {
                materialized_values: 4,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
}

#[test]
fn typed_matrix_controls_preserve_defaults_selector_order_and_error_precedence() {
    // These small permutations and padding coordinates follow the documented transform contracts.
    // GROUPBY's consumed header is excluded from its filter mask and sort applies to key columns.
    let text = CellValue::TextUtf16(ExcelText::from_units(vec![0xd800, 0x61]));
    let cases = [
        ("TAKE({1,2;3,4},,1)", "D11", CellValue::Number(3.)),
        (
            "TAKE({1,2;3,4},B1,1)",
            "D10",
            CellValue::Error(CellError::Calc),
        ),
        ("DROP({1,2;3,4},,-1)", "D11", CellValue::Number(3.)),
        ("EXPAND({1,2},B1,3,9)", "F10", CellValue::Number(9.)),
        ("WRAPROWS({1,2,3},2,A1)", "E11", text.clone()),
        ("WRAPCOLS({1,2,3},2,A1)", "E11", text.clone()),
        ("TOCOL({1,#N/A;3,4},2,TRUE)", "D12", CellValue::Number(4.)),
        ("TOROW({1,#N/A;3,4},2,TRUE)", "F10", CellValue::Number(4.)),
        ("CHOOSECOLS({1,2;3,4},{2,1})", "E11", CellValue::Number(3.)),
        ("CHOOSEROWS({1,2;3,4},{2;1})", "E11", CellValue::Number(2.)),
        ("SORT({2,1;4,3},1,-1,TRUE)", "E11", CellValue::Number(3.)),
        (
            "SORTBY({10;20;30},{3;1;2},-1)",
            "D12",
            CellValue::Number(20.),
        ),
        ("FILTER({1,2;3,4},{0;1})", "E10", CellValue::Number(4.)),
        ("MUNIT(\"2\")", "E11", CellValue::Number(1.)),
        (
            "GROUPBY({\"g\";\"b\";\"a\";\"b\"},{0;1;2;3},SUM,1,0,-1,{#N/A;TRUE;TRUE;TRUE})",
            "E11",
            CellValue::Number(2.),
        ),
        (
            "GROUPBY({\"b\";\"a\"},{1;2},SUM,0,0,{0,#N/A})",
            "D10",
            CellValue::Error(CellError::Value),
        ),
        (
            "FILTER({1,2,3;4,5,6},{1,#N/A})",
            "D10",
            CellValue::Error(CellError::Value),
        ),
        (
            "TAKE({1,2;3,4},#N/A)",
            "D10",
            CellValue::Error(CellError::NA),
        ),
    ];
    for (formula, child, expected) in cases {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet.set_value(at("A1").address, text.clone()).unwrap();
        sheet.set_formula(at("D10").address, formula).unwrap();
        let context = EvaluationContext::default();
        let mut direct = Evaluation::with_workbook(&book)
            .build(context.clone())
            .unwrap();
        assert_eq!(direct.cell(at(child)).unwrap(), expected, "{formula}");
        let mut overlay = Overlay::with_workbook(&book).build().unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at(child))
                .unwrap(),
            expected,
            "{formula}"
        );
    }
}
