//! Non-DBCS legacy byte text contracts, independently derived from Microsoft:
//! https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/7a8bce94-30cb-44a6-a2a5-e4b2ac26eab3
//! https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/c233386c-f59b-4160-b9cb-7c5df7623b72
//! https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/d8a4dbca-7634-4282-90d9-59b04c5b00cf
//! https://support.microsoft.com/en-us/excel/compatibility-versions
//! Microsoft distinguishes default-language byte counting from UTF-8 and limits
//! V2 surrogate changes to ordinary LEN/MID/FIND/SEARCH/REPLACE, not B functions.
//! No upstream code, test data, or algorithms were copied.
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::eval::{
    ByteTextLanguage, EvaluationBudgets, EvaluationContext, EvaluationError, EvaluationOutcome,
    EvaluationPolicy, EvaluationTarget, RunConfiguration, run_formula_with_configuration,
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
    language: ByteTextLanguage,
) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        book,
        0,
        CellAddress { row: 9, col: 9 },
        formula,
        &RunConfiguration::default().with_byte_text_language(language),
        Default::default(),
    )
}

fn value(formula: &str) -> CellValue {
    match evaluate(&book(), formula, ByteTextLanguage::NonDbcs) {
        EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("{formula}: {outcome:?}"),
    }
}

#[test]
fn non_dbcs_counts_legacy_units_across_scripts_and_coercions() {
    // BMP characters consume one non-DBCS position, independently of the bytes
    // needed to encode them as UTF-8 or a regional Windows codepage.
    for (formula, expected) in [
        ("LENB(\"Aé中ｶאב\")", 6.),
        ("LENB(\"😀\")", 2.),
        ("LENB(\"\")", 0.),
        ("LENB(TRUE)", 4.),
        ("LENB(123)", 3.),
        ("LENB(A1)", 0.),
        ("_xlfn.LENB(\"中\")", 1.),
        ("FINDB(\"中\",\"Aé中ｶ\")", 3.),
        ("FINDB(\"B\",\"😀B\")", 3.),
        ("SEARCHB(\"é\",\"AÉ中\")", 2.),
        ("INDEX(LENB({\"中\",\"😀\"}),1,2)", 2.),
    ] {
        assert_eq!(value(formula), CellValue::Number(expected), "{formula}");
    }
}

#[test]
fn slicing_and_replacement_validate_positions_clamp_and_preserve_units() {
    for (formula, expected) in [
        ("LEFTB(\"é中ｶ\")", "é"),
        ("RIGHTB(\"é中ｶ\")", "ｶ"),
        ("MIDB(\"é中ｶ\",2,1)", "中"),
        ("LEFTB(\"中\",2)", "中"),
        ("RIGHTB(\"中\",99)", "中"),
        ("MIDB(\"中\",99,99)", ""),
        ("MIDB(\"abc\",2,2147483647)", "bc"),
        ("LEFTB(\"中\",0)", ""),
        ("RIGHTB(\"中\",0)", ""),
        ("MIDB(\"中\",1,0)", ""),
        ("LEFTB(\"abc\",1.9)", "a"),
        ("REPLACEB(\"Aé中\",2,1,\"x\")", "Ax中"),
        ("REPLACEB(\"abc\",99,99,\"x\")", "abcx"),
        ("REPLACEB(\"abc\",2,0,\"x\")", "axbc"),
        ("REPLACEB(\"abc\",2,99,\"x\")", "ax"),
        ("INDEX(LEFTB({\"é中\",\"ｶA\"}),1,2)", "ｶ"),
    ] {
        assert_eq!(
            value(formula),
            CellValue::Text(expected.into()),
            "{formula}"
        );
    }
    for (formula, expected) in [
        ("LEFTB(\"😀\",1)", vec![0xd83d]),
        ("RIGHTB(\"😀\",1)", vec![0xde00]),
        ("MIDB(\"😀\",2,1)", vec![0xde00]),
        ("REPLACEB(\"😀\",1,1,\"X\")", vec![0x58, 0xde00]),
    ] {
        assert_eq!(
            value(formula)
                .text_ref()
                .unwrap()
                .units()
                .collect::<Vec<_>>(),
            expected,
            "{formula}"
        );
    }
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_value(
            "A1",
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800, 0x4e2d, 0xdc00])),
        )
        .unwrap();
    for (formula, units) in [
        ("LEFTB(A1,1)", vec![0xd800]),
        ("RIGHTB(A1,1)", vec![0xdc00]),
        ("MIDB(A1,2,1)", vec![0x4e2d]),
    ] {
        let EvaluationOutcome::Complete { value, .. } =
            evaluate(&book, formula, ByteTextLanguage::NonDbcs)
        else {
            panic!("{formula}")
        };
        assert_eq!(value.text_ref().unwrap().units().collect::<Vec<_>>(), units);
    }
}

#[test]
fn find_is_literal_search_is_case_insensitive_with_wildcards_and_escapes() {
    for (formula, expected) in [
        ("FINDB(\"a\",\"Aba\")", 3.),
        ("FINDB(\"*\",\"ab*c\")", 3.),
        ("FINDB(\"a\",\"aaa\",2)", 2.),
        ("FINDB(\"\",\"abc\",2)", 2.),
        ("SEARCHB(\"a?C\",\"xxAbC\")", 3.),
        ("SEARCHB(\"~*\",\"ab*c\")", 3.),
        ("SEARCHB(\"~?\",\"ab?c\")", 3.),
        ("SEARCHB(\"a*c\",\"xxAb中C\")", 3.),
        ("SEARCHB(\"?B\",\"😀B\")", 2.),
        ("SEARCHB(\"B\",\"😀B\",2)", 3.),
    ] {
        assert_eq!(value(formula), CellValue::Number(expected), "{formula}");
    }
}

#[test]
fn invalid_counts_positions_arity_and_source_errors_are_spreadsheet_errors() {
    for formula in [
        "LENB()",
        "LENB(1,2)",
        "LEFTB(\"a\",-1)",
        "RIGHTB(\"a\",-1)",
        "MIDB(\"a\",0,1)",
        "MIDB(\"a\",1,-1)",
        "REPLACEB(\"a\",0,1,\"x\")",
        "REPLACEB(\"a\",1,-1,\"x\")",
        "FINDB(\"a\",\"a\",0)",
        "SEARCHB(\"a\",\"a\",0)",
        "FINDB(\"a\",\"a\",2)",
        "SEARCHB(\"a\",\"a\",2)",
        "FINDB(\"b\",\"a\")",
        "SEARCHB(\"b\",\"a\")",
    ] {
        assert_eq!(
            value(formula),
            CellValue::Error(CellError::Value),
            "{formula}"
        );
    }
    for name in ["LENB", "LEFTB", "RIGHTB"] {
        assert_eq!(
            value(&format!("{name}(#N/A)")),
            CellValue::Error(CellError::NA)
        );
    }
    assert_eq!(
        value("REPLACEB(\"a\",1,1,#DIV/0!)"),
        CellValue::Error(CellError::Div0)
    );
}

#[test]
fn byte_functions_keep_legacy_units_alongside_modern_text_and_host_locale() {
    let book = book();
    let non_dbcs = RunConfiguration::default();
    let dbcs = non_dbcs
        .clone()
        .with_byte_text_language(ByteTextLanguage::Dbcs);
    assert_ne!(non_dbcs.fingerprint(), dbcs.fingerprint());
    // Only cells that read the byte-text language, and their dependents, become stale.
    assert_eq!(
        non_dbcs.fingerprint().computation(),
        dbcs.fingerprint().computation()
    );
    assert_eq!(value("LENB(\"😀\")"), CellValue::Number(2.));
    assert_eq!(value("LEN(\"😀\")"), CellValue::Number(1.));
    let config = RunConfiguration::default().with_number_locale(
        poirot::eval::NumberLocale::new(",", Some("."), Some("EUR"), "%", ":").unwrap(),
    );
    let result = run_formula_with_configuration(
        &book,
        0,
        "J10".parse().unwrap(),
        "LENB(\"é中\")",
        &config,
        Default::default(),
    );
    assert!(matches!(
        result,
        EvaluationOutcome::Complete {
            value: CellValue::Number(2.),
            ..
        }
    ));
}

#[test]
fn dbcs_and_budget_refusals_are_operational_and_cannot_be_masked() {
    let book = book();
    for formula in [
        "IFERROR(LENB(\"中\"),99)",
        "LEFTB(\"中\",1)",
        "RIGHTB(\"中\",1)",
        "MIDB(\"中\",1,1)",
        "FINDB(\"中\",\"中\")",
        "SEARCHB(\"中\",\"中\")",
        "REPLACEB(\"中\",1,1,\"x\")",
    ] {
        assert!(
            matches!(
                evaluate(&book, formula, ByteTextLanguage::Dbcs),
                EvaluationOutcome::Aborted {
                    failure: EvaluationError::UnsupportedCapability {
                        reason: poirot::eval::EvaluationReasonError::DbcsByteText,
                        ..
                    },
                    ..
                }
            ),
            "{formula}"
        );
    }
    for (formula, text_bytes) in [
        ("IFERROR(LEFTB(\"abcdef\",5),99)", 10),
        ("IFERROR(REPLACEB(\"abc\",2,1,\"defgh\"),99)", 12),
        ("IFERROR(SEARCHB(\"a*c\",\"aaaaac\"),99)", 12),
    ] {
        let result = run_formula_with_configuration(
            &book,
            0,
            "J10".parse().unwrap(),
            formula,
            &RunConfiguration::default(),
            EvaluationBudgets {
                text_bytes,
                ..Default::default()
            },
        );
        assert!(
            matches!(
                result,
                EvaluationOutcome::Aborted {
                    failure: EvaluationError::Budget(_),
                    ..
                }
            ),
            "{formula}: {result:?}"
        );
    }
    let result = run_formula_with_configuration(
        &book,
        0,
        "J10".parse().unwrap(),
        "IFERROR(FINDB(\"zzz\",\"aaaaaaaaaaa\"),99)",
        &RunConfiguration::default(),
        EvaluationBudgets {
            run_work: 1,
            ..Default::default()
        },
    );
    assert!(
        matches!(
            result,
            EvaluationOutcome::Aborted {
                failure: EvaluationError::Budget(_),
                ..
            }
        ),
        "{result:?}"
    );
}

#[test]
fn overlay_invalidates_consumers_when_byte_language_changes_and_recovers() {
    let mut book = book();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_formula("A1", "LENB(\"😀\")").unwrap();
    sheet.set_formula("B1", "A1+1").unwrap();
    sheet.set_formula("C1", "IF(FALSE,LENB(\"中\"),7)").unwrap();
    sheet.set_formula("D1", "LEN(\"😀\")").unwrap();
    let target = |name: &str| EvaluationTarget {
        sheet: 0,
        address: name.parse().unwrap(),
    };
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B1"))
            .unwrap(),
        CellValue::Number(3.)
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C1"))
            .unwrap(),
        CellValue::Number(7.)
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("D1"))
            .unwrap(),
        CellValue::Number(1.)
    );
    let mut policy = overlay.policy().clone();
    policy.byte_text_language = ByteTextLanguage::Dbcs;
    overlay.set_policy(policy).unwrap();
    assert!(overlay.cached_value(target("A1")).is_none());
    assert!(overlay.cached_value(target("B1")).is_none());
    assert!(overlay.cached_value(target("C1")).is_some());
    assert!(overlay.cached_value(target("D1")).is_some());
    assert!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B1"))
            .is_err()
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C1"))
            .unwrap(),
        CellValue::Number(7.)
    );
    let mut policy = overlay.policy().clone();
    policy.byte_text_language = ByteTextLanguage::NonDbcs;
    overlay.set_policy(policy).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B1"))
            .unwrap(),
        CellValue::Number(3.)
    );
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    assert_eq!(direct.cell(target("A1")).unwrap(), CellValue::Number(2.));
    direct
        .set_policy(EvaluationPolicy {
            byte_text_language: ByteTextLanguage::Dbcs,
            ..Default::default()
        })
        .unwrap();
    assert!(matches!(
        direct.cell(target("A1")),
        Err(EvaluationError::UnsupportedCapability { .. })
    ));
    direct.set_policy(EvaluationPolicy::default()).unwrap();
    direct.begin(context).unwrap();
    assert_eq!(direct.cell(target("A1")).unwrap(), CellValue::Number(2.));
}
