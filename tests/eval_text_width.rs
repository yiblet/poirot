//! Japanese width contract: Microsoft documents ASC and DBCS/JIS kana conversion.
//! https://support.microsoft.com/ja-jp/excel/functions/asc-function
//! https://support.microsoft.com/ja-jp/excel/functions/jis-function
//! https://support.microsoft.com/en-au/excel/functions/dbcs-function
//! Additional expectations follow selected Unicode 17.0.0 width/canonical mappings,
//! not live Excel measurements; no upstream tests or implementation were copied.
use poirot::eval::{
    EvaluationBudgets, EvaluationOutcome, RunConfiguration, run_formula_with_configuration,
};
use poirot::model::{CellAddress, CellError, CellValue, ExcelText, Workbook};

fn book() -> Workbook {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    book
}

fn evaluate(
    book: &Workbook,
    formula: &str,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        book,
        0,
        CellAddress { row: 0, col: 10 },
        formula,
        &RunConfiguration::default(),
        budgets,
    )
}

fn value(formula: &str) -> CellValue {
    match evaluate(&book(), formula, Default::default()) {
        EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("{formula}: {outcome:?}"),
    }
}

#[test]
fn converts_documented_japanese_kana_and_width_pairs() {
    for (formula, expected) in [
        ("ASC(\"エクセル\")", "ｴｸｾﾙ"),
        ("DBCS(\"ｴｸｾﾙ\")", "エクセル"),
        ("JIS(\"ｴｸｾﾙ\")", "エクセル"),
        ("ASC(\"ＡＺａｚ０９！～　。「」、・ー\")", "AZaz09!~ ｡｢｣､･ｰ"),
        (
            "DBCS(\"AZaz09!~ ｡｢｣､･ｰ\")",
            "ＡＺａｚ０９！～　。「」、・ー",
        ),
        (
            "ASC(\"ガギグゲゴザジズゼゾダヂヅデドバビブベボパピプペポヴヷヺ\")",
            "ｶﾞｷﾞｸﾞｹﾞｺﾞｻﾞｼﾞｽﾞｾﾞｿﾞﾀﾞﾁﾞﾂﾞﾃﾞﾄﾞﾊﾞﾋﾞﾌﾞﾍﾞﾎﾞﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟｳﾞﾜﾞｦﾞ",
        ),
        (
            "DBCS(\"ｶﾞｷﾞｸﾞｹﾞｺﾞｻﾞｼﾞｽﾞｾﾞｿﾞﾀﾞﾁﾞﾂﾞﾃﾞﾄﾞﾊﾞﾋﾞﾌﾞﾍﾞﾎﾞﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟｳﾞﾜﾞｦﾞ\")",
            "ガギグゲゴザジズゼゾダヂヅデドバビブベボパピプペポヴヷヺ",
        ),
        ("ASC(\"カ\u{3099}ハ\u{309a}゛゜\")", "ｶﾞﾊﾟﾞﾟ"),
        ("DBCS(\"ﾞﾟｱﾞｶﾟ\")", "゛゜ア゛カ゜"),
        ("ASC(\"漢あéאב😀①ﬃK\")", "漢あéאב😀①ﬃK"),
        ("DBCS(\"漢あéאב😀①ﬃK\")", "漢あéאב😀①ﬃK"),
        ("ASC(42)", "42"),
        ("DBCS(42)", "４２"),
        ("ASC(TRUE)", "TRUE"),
        ("DBCS(TRUE)", "ＴＲＵＥ"),
        ("ASC(A1)", ""),
        ("DBCS(\"\")", ""),
        ("INDEX(JIS({\"a\",\"ｶﾞ\"}),1,2)", "ガ"),
        ("INDEX(ASC({\"Ａ\",\"パ\"}),1,2)", "ﾊﾟ"),
    ] {
        assert_eq!(
            value(formula),
            CellValue::Text(expected.into()),
            "{formula}"
        );
    }
}

#[test]
fn preserves_unpaired_utf16_without_replacement() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_value(
            CellAddress { row: 0, col: 0 },
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800, 0xff21, 0xdc00])),
        )
        .unwrap();
    for (formula, middle) in [("ASC(A1)", 0x41), ("DBCS(A1)", 0xff21)] {
        let EvaluationOutcome::Complete { value, .. } =
            evaluate(&book, formula, Default::default())
        else {
            panic!("{formula}")
        };
        assert_eq!(
            value,
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800, middle, 0xdc00]))
        );
    }
}

#[test]
fn propagates_errors_checks_arity_and_expansion_limit() {
    for (formula, expected) in [
        ("ASC()", CellError::Value),
        ("DBCS(\"a\",\"b\")", CellError::Value),
        ("JIS(#N/A)", CellError::NA),
        ("ASC(1/0)", CellError::Div0),
        ("ASC(REPT(\"ガ\",16384))", CellError::Value),
    ] {
        assert_eq!(value(formula), CellValue::Error(expected), "{formula}");
    }
    assert_eq!(
        value("LEN(ASC(REPT(\"ガ\",16383)&\"Ａ\"))"),
        CellValue::Number(32767.0)
    );
    assert_eq!(
        value("LEN(DBCS(REPT(\"a\",32767)))"),
        CellValue::Number(32767.0)
    );
}

#[test]
fn allocation_and_work_refusals_remain_host_failures() {
    for budgets in [
        EvaluationBudgets {
            text_bytes: 12,
            ..Default::default()
        },
        EvaluationBudgets {
            run_work: 1,
            ..Default::default()
        },
    ] {
        let result = evaluate(&book(), "IFERROR(ASC(\"ガガガガ\"),99)", budgets);
        assert!(
            matches!(result, EvaluationOutcome::Aborted { .. }),
            "{result:?}"
        );
    }
}
