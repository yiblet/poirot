//! Optional lookup operands preserve absence independently of blank values.
use poirot::calculation::EvaluationContext;
use poirot::eval::BuiltinSemanticsPolicy;
use poirot::eval::EvaluationPolicy;
use poirot::eval::overlay::Evaluation;
use poirot::eval::overlay::Overlay;
use poirot::model::Workbook;
use poirot::{CellError, CellValue};

#[test]
fn lookup_optional_modes_and_fallbacks_preserve_public_argument_channels() {
    // Microsoft documents exact match0/forward search1 and NA for missing fallback.
    // https://support.microsoft.com/en-us/excel/functions/xlookup-function
    // https://support.microsoft.com/en-gb/excel/functions/xmatch-function?nochrome=true
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    for (address, value) in [
        ("A1", 2.),
        ("A2", 1.),
        ("A3", 2.),
        ("B1", 20.),
        ("B2", 10.),
        ("B3", 21.),
    ] {
        sheet.set_value(address.parse().unwrap(), value).unwrap();
    }
    let context = EvaluationContext::default();
    for builtin in [
        BuiltinSemanticsPolicy::ExcelDocumentation,
        BuiltinSemanticsPolicy::PinnedPoi,
    ] {
        let policy = EvaluationPolicy {
            builtin,
            ..Default::default()
        };
        let mut direct = Evaluation::with_workbook(&book)
            .with_policy(policy.clone())
            .build(context.clone())
            .unwrap();
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(policy)
            .build()
            .unwrap();
        let target = direct.target("Data!D1").unwrap();
        for (formula, expected) in [
            ("XLOOKUP(2,A1:A3,B1:B3)", CellValue::Number(20.)),
            ("XLOOKUP(2,A1:A3,B1:B3,,)", CellValue::Number(20.)),
            ("XLOOKUP(2,A1:A3,B1:B3,,,)", CellValue::Number(20.)),
            ("XLOOKUP(2,A1:A3,B1:B3,,,-1)", CellValue::Number(21.)),
            ("XLOOKUP(2,A1:A3,B1:B3,,0,)", CellValue::Number(20.)),
            ("XLOOKUP(2,A1:A3,B1:B3,,G1,-1)", CellValue::Number(21.)),
            (
                "XLOOKUP(2,A1:A3,B1:B3,,0,G1)",
                CellValue::Error(CellError::Value),
            ),
            (
                "XLOOKUP(2,A1:A3,B1:B3,,,0)",
                CellValue::Error(CellError::Value),
            ),
            (
                "XLOOKUP(2,A1:A3,B1:B3,,1/0)",
                CellValue::Error(CellError::Div0),
            ),
            (
                "XLOOKUP(2,A1:A3,B1:B3,,,#N/A)",
                CellValue::Error(CellError::NA),
            ),
            (
                "XLOOKUP(2,A1:A3,B1:B3,1/0,,)",
                CellValue::Error(CellError::Div0),
            ),
            ("XLOOKUP(9,A1:A3,B1:B3)", CellValue::Error(CellError::NA)),
            ("XLOOKUP(9,A1:A3,B1:B3,)", CellValue::Error(CellError::NA)),
            (
                "XLOOKUP(9,A1:A3,B1:B3,,,1)",
                CellValue::Error(CellError::NA),
            ),
            ("XLOOKUP(9,A1:A3,B1:B3,0,,)", CellValue::Number(0.)),
            ("XLOOKUP(9,A1:A3,B1:B3,G1,,)", CellValue::Number(0.)),
            (r#"XLOOKUP(9,A1:A3,B1:B3,"",,)"#, CellValue::Text("".into())),
            (
                r#"XLOOKUP(9,A1:A3,B1:B3,"none",,)"#,
                CellValue::Text("none".into()),
            ),
            ("XLOOKUP(2,{2;1;2},B1:B3,,,-1)", CellValue::Number(21.)),
            ("XMATCH(2,A1:A3)", CellValue::Number(1.)),
            ("XMATCH(2,A1:A3,)", CellValue::Number(1.)),
            ("XMATCH(2,A1:A3,,)", CellValue::Number(1.)),
            ("XMATCH(2,A1:A3,,-1)", CellValue::Number(3.)),
            ("XMATCH(2,A1:A3,0,)", CellValue::Number(1.)),
            ("XMATCH(2,A1:A3,,G1)", CellValue::Error(CellError::Value)),
            ("XMATCH(2,A1:A3,,0)", CellValue::Error(CellError::Value)),
            ("XMATCH(2,A1:A3,1/0,)", CellValue::Error(CellError::Div0)),
            ("XMATCH(9,A1:A3,,)", CellValue::Error(CellError::NA)),
            ("XMATCH(2,{1;2;3},,2)", CellValue::Number(2.)),
        ] {
            direct.set_formula(target, formula).unwrap();
            overlay.set_formula(target, formula).unwrap();
            assert_eq!(
                direct.cell(target).unwrap(),
                expected,
                "{builtin:?}: {formula}"
            );
            assert_eq!(
                overlay.evaluation(&context).unwrap().cell(target).unwrap(),
                expected,
                "{builtin:?}: {formula}"
            );
        }
    }
}
