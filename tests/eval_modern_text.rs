//! Independently authored modern text contracts, based on Microsoft documentation.
//! https://support.microsoft.com/en-us/excel/functions/textbefore-function
//! https://support.microsoft.com/en-us/excel/functions/textafter-function
//! https://support.microsoft.com/en-us/excel/functions/textsplit-function
//! https://support.microsoft.com/en-us/excel/functions/arraytotext-function
//! https://support.microsoft.com/en-us/excel/functions/valuetotext-function
use poirot::eval::{
    overlay::{Evaluation, EvaluationContext, EvaluationPolicy, Overlay},
    *,
};
use poirot::model::{CellError, CellValue, ExcelText, Workbook};
fn at(s: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: s.parse().unwrap(),
    }
}
fn book() -> Workbook {
    let mut b = Workbook::new();
    b.add_sheet("S").unwrap();
    b
}
fn formula(f: &str) -> CellValue {
    match run_formula(
        &book(),
        0,
        at("Z100").address,
        f,
        Default::default(),
        Default::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{f}: {other:?}"),
    }
}
#[test]
fn extraction_defaults_signed_occurrences_modes_and_boundaries() {
    for (f, s) in [
        ("TEXTBEFORE(\"oak::elm::ash\",\"::\")", "oak"),
        ("TEXTAFTER(\"oak::elm::ash\",\"::\")", "elm::ash"),
        ("TEXTBEFORE(\"oak::elm::ash\",\"::\",-1)", "oak::elm"),
        ("TEXTAFTER(\"oak::elm::ash\",\"::\",-2)", "elm::ash"),
        ("TEXTBEFORE(\"oak\",\"?\",,,1)", "oak"),
        ("TEXTAFTER(\"oak\",\"?\",,,1)", ""),
        ("TEXTAFTER(\"oak\",\"?\",-1,,1)", "oak"),
        ("TEXTBEFORE(\"Ä:leaf\",\"ä:\",,1)", ""),
        ("TEXTAFTER(\"Ä:leaf\",\"ä:\",,1)", "leaf"),
        ("TEXTBEFORE(\"oak\",\"\")", ""),
        ("TEXTBEFORE(\"oak\",\"\",-1)", "oak"),
        ("TEXTAFTER(\"oak\",\"\")", "oak"),
        ("TEXTAFTER(\"oak\",\"\",-1)", ""),
        ("TEXTBEFORE(\"\",\"?\")", ""),
        ("TEXTBEFORE(\"oak\",\"?\",1,,,\"missing\")", "missing"),
        ("TEXTAFTER(\"a*b\",\"*\")", "b"),
    ] {
        assert_eq!(formula(f), CellValue::Text(s.into()), "{f}");
    }
}
#[test]
fn extraction_failures_and_optional_blank_are_distinct() {
    for (f, e) in [
        ("TEXTBEFORE(\"oak\",\"?\")", CellError::NA),
        ("TEXTAFTER(\"oak\",\"?\",0)", CellError::Value),
        ("TEXTAFTER(\"oak\",\"?\",4,,,1/0)", CellError::Value),
        ("TEXTBEFORE(\"a:b\",\":\",A1)", CellError::Value),
        ("TEXTBEFORE(\"a:b\",\":\",,2)", CellError::Value),
        ("TEXTBEFORE(\"a:b\",\":\",,,2)", CellError::Value),
        ("TEXTAFTER(#DIV/0!,\":\")", CellError::Div0),
        ("TEXTBEFORE()", CellError::Value),
    ] {
        assert_eq!(formula(f), CellValue::Error(e), "{f}");
    }
    assert_eq!(
        formula("TEXTBEFORE(\"a:b\",\":\",,,,,)"),
        CellValue::Error(CellError::Value)
    );
    assert_eq!(
        formula("TEXTBEFORE(\"a:b\",\":\",,,,1/0)"),
        CellValue::Text("a".into())
    );
    assert_eq!(
        formula("TEXTBEFORE(\"abc\",\":\",,,,1/0)"),
        CellValue::Error(CellError::Div0)
    );
}
#[test]
fn split_two_axes_sets_padding_empty_fields_and_row_only_omission() {
    for (f, s) in [
        ("INDEX(TEXTSPLIT(\"a,b;c\",\",\",\";\"),1,2)", "b"),
        (
            "INDEX(TEXTSPLIT(\"a,b;c\",\",\",\";\",,,\"pad\"),2,2)",
            "pad",
        ),
        ("INDEX(TEXTSPLIT(\"a;;b\",,\";\"),2,1)", ""),
        ("INDEX(TEXTSPLIT(\"a;;b\",,\";\",TRUE),2,1)", "b"),
        ("INDEX(TEXTSPLIT(\"a|b,c\",{\"|\",\",\"}),1,3)", "c"),
        ("INDEX(TEXTSPLIT(\"aÄb\",\"ä\",,,1),1,2)", "b"),
        ("INDEX(TEXTSPLIT(\"a,,b,\",\",\"),1,2)", ""),
        ("INDEX(TEXTSPLIT(\"a,,b,\",\",\",,TRUE),1,2)", "b"),
    ] {
        assert_eq!(formula(f), CellValue::Text(s.into()), "{f}");
    }
    assert_eq!(
        formula("INDEX(TEXTSPLIT(\"a,b;c\",\",\",\";\"),2,2)"),
        CellValue::Error(CellError::NA)
    );
    assert_eq!(
        formula("INDEX(TEXTSPLIT(\"a,b;c\",\",\",\";\",,,J1),2,2)"),
        0.0.into()
    );
}
#[test]
fn split_domain_errors_and_nested_array_refusal() {
    for (f, e) in [
        ("TEXTSPLIT(\"a\",\"\")", CellError::Value),
        ("TEXTSPLIT(\"a\",)", CellError::Value),
        ("TEXTSPLIT(\"a,b\",\",\",,,2)", CellError::Value),
        ("TEXTSPLIT(\",,\",\",\",,TRUE)", CellError::Calc),
        ("TEXTSPLIT({\"a,b\";\"c,d\"},\",\")", CellError::Value),
        ("TEXTSPLIT(\"a,b\",#REF!)", CellError::Ref),
    ] {
        assert_eq!(formula(f), CellValue::Error(e), "{f}");
    }
}
#[test]
fn serializers_preserve_types_escape_text_and_aggregate_geometry() {
    for (f, s) in [
        (
            "ARRAYTOTEXT({7,TRUE;\"pine\",#DIV/0!})",
            "7, TRUE, pine, #DIV/0!",
        ),
        (
            "ARRAYTOTEXT({7,TRUE;\"pine\",#DIV/0!},1)",
            "{7,TRUE;\"pine\",#DIV/0!}",
        ),
        ("VALUETOTEXT(\"pine\")", "pine"),
        ("VALUETOTEXT(\"pine\",1)", "\"pine\""),
        ("VALUETOTEXT(\"a\"\"b\",1)", "\"a\"\"b\""),
        ("VALUETOTEXT(#N/A)", "#N/A"),
        ("VALUETOTEXT(FALSE,1)", "FALSE"),
        ("VALUETOTEXT(7.25,1)", "7.25"),
        (
            "INDEX(VALUETOTEXT({7,TRUE;\"pine\",#N/A},1),2,1)",
            "\"pine\"",
        ),
        ("INDEX(VALUETOTEXT(\"fir\",{0,1}),1,2)", "\"fir\""),
        ("ARRAYTOTEXT(A1:B1,1)", "{,}"),
    ] {
        assert_eq!(formula(f), CellValue::Text(s.into()), "{f}");
    }
    for f in [
        "VALUETOTEXT(1,2)",
        "ARRAYTOTEXT({1,2},0.5)",
        "VALUETOTEXT()",
    ] {
        assert_eq!(formula(f), CellValue::Error(CellError::Value), "{f}");
    }
}
#[test]
fn input_columns_and_optional_arguments_broadcast_in_both_evaluators() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("A1").address, "x/y").unwrap();
    s.set_value(at("A2").address, "p/q").unwrap();
    for (cell, f) in [
        ("D1", "TEXTAFTER(A1:A2,\"/\")"),
        ("H1", "TEXTBEFORE({\"a:b\";\"c:d\"},\":\",{1,-1})"),
        ("L1", "VALUETOTEXT({2,TRUE;#N/A,\"x\"},1)"),
        (
            "P1",
            "TEXTBEFORE({\"found:x\";\"missing\"},\":\",,,,{\"one\",\"two\"})",
        ),
    ] {
        s.set_formula(at(cell).address, f).unwrap();
    }
    let mut direct = Evaluation::with_workbook(&b)
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    let context = EvaluationContext::default();
    let mut e = overlay.evaluation(&context).unwrap();
    for (cell, value) in [
        ("D2", "q"),
        ("I2", "c"),
        ("M2", "\"x\""),
        ("L2", "#N/A"),
        ("Q1", "found"),
        ("P2", "one"),
        ("Q2", "two"),
    ] {
        assert_eq!(
            direct.cell(at(cell)).unwrap(),
            CellValue::Text(value.into()),
            "direct {cell}"
        );
        assert_eq!(
            e.cell(at(cell)).unwrap(),
            CellValue::Text(value.into()),
            "overlay {cell}"
        );
    }
}
#[test]
fn split_spills_child_first_obstructions_and_overlay_edits() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("J1").address, "a,b;c,d").unwrap();
    s.set_formula(at("A1").address, "TEXTSPLIT(J1,\",\",\";\")")
        .unwrap();
    s.set_formula(at("D1").address, "TEXTSPLIT(\"a,b\",\",\")")
        .unwrap();
    s.set_value(at("E1").address, 9.0).unwrap();
    s.set_formula(at("H1").address, "TEXTBEFORE(\"a:b\",\":\",,,,H1)")
        .unwrap();
    let mut direct = Evaluation::with_workbook(&b)
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&b)
        .with_policy(EvaluationPolicy::default())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    for (cell, value) in [
        ("B2", CellValue::Text("d".into())),
        ("D1", CellValue::Error(CellError::Spill)),
        ("H1", CellValue::Text("a".into())),
    ] {
        assert_eq!(direct.cell(at(cell)).unwrap(), value);
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at(cell))
                .unwrap(),
            value
        );
    }
    overlay.set_value(at("J1"), "single").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B2"))
            .unwrap(),
        CellValue::Blank
    );
    overlay.set_value(at("J1"), "u,v;w,z").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B2"))
            .unwrap(),
        CellValue::Text("z".into())
    );
}
#[test]
fn all_paths_preserve_unpaired_utf16_units() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(
        at("A1").address,
        CellValue::TextUtf16(ExcelText::from_units(vec![0xd800, 58, 0xdc00])),
    )
    .unwrap();
    for (cell, f) in [
        ("D1", "TEXTBEFORE(A1,\":\")"),
        ("F1", "TEXTAFTER(A1,\":\")"),
        ("H1", "TEXTSPLIT(A1,\":\")"),
        ("L1", "VALUETOTEXT(A1,1)"),
    ] {
        s.set_formula(at(cell).address, f).unwrap();
    }
    let mut direct = Evaluation::with_workbook(&b)
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    let c = EvaluationContext::default();
    let mut e = overlay.evaluation(&c).unwrap();
    for (cell, units) in [
        ("D1", vec![0xd800]),
        ("F1", vec![0xdc00]),
        ("I1", vec![0xdc00]),
        ("L1", vec![34, 0xd800, 58, 0xdc00, 34]),
    ] {
        let expected = CellValue::TextUtf16(ExcelText::from_units(units));
        assert_eq!(direct.cell(at(cell)).unwrap(), expected);
        assert_eq!(e.cell(at(cell)).unwrap(), expected);
    }
}
#[test]
fn operational_limits_are_not_catchable_spreadsheet_errors() {
    let b = book();
    for f in [
        "IFERROR(TEXTSPLIT(\"a,b,c,d\",\",\"),99)",
        "IFERROR(ARRAYTOTEXT({1,2;3,4},1),99)",
        "IFERROR(TEXTAFTER(\"abcdef\",\"f\"),99)",
    ] {
        let result = run_formula_with_configuration(
            &b,
            0,
            at("Z100").address,
            f,
            &RunConfiguration::default(),
            EvaluationBudgets {
                materialized_values: 3,
                ..Default::default()
            },
        );
        assert!(
            matches!(result, EvaluationOutcome::Aborted { .. }),
            "{f}: {result:?}"
        );
    }
}

#[test]
fn modern_text_registration_preserves_the_explicit_pinned_registry() {
    let b = book();
    for name in [
        "TEXTBEFORE",
        "TEXTAFTER",
        "TEXTSPLIT",
        "ARRAYTOTEXT",
        "VALUETOTEXT",
    ] {
        let f = format!("{name}(\"a:b\",\":\")");
        let result = run_formula_with_configuration(
            &b,
            0,
            at("Z100").address,
            &f,
            &RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi),
            Default::default(),
        );
        assert!(
            matches!(
                result,
                EvaluationOutcome::Complete {
                    value: CellValue::Error(CellError::Name),
                    ..
                }
            ),
            "{f}: {result:?}"
        );
    }
}

#[test]
fn scalar_text_kernels_spill_child_first_with_optional_argument_arrays() {
    // Microsoft function reference defines LEN's character count, LEFT's optional
    // count, EXACT's Boolean comparison, and NUMBERVALUE's explicit separators.
    // https://support.microsoft.com/en-us/office/text-functions-reference-cccd86ad-547d-4ea9-a065-7bb697c2a56e
    let mut b = book();
    let sheet = b.sheet_mut(0).unwrap();
    for (cell, expression) in [
        ("A1", "LEN({\"a\";\"abcd\"})"),
        ("D1", "LEFT(\"abcd\",{1,3})"),
        ("G1", "EXACT({\"a\";\"A\"},\"a\")"),
        ("J1", "NUMBERVALUE({\"1.5\";\"2.5\"},\".\",\",\")"),
        ("M1", "CHAR({65,66})"),
        ("P1", "SUBSTITUTE({\"a-a\";\"b-b\"},\"-\",\"/\")"),
        ("S1", "CONCATENATE({\"a\";\"b\"},{1,2})"),
    ] {
        sheet.set_formula(at(cell).address, expression).unwrap();
    }
    let mut direct = Evaluation::with_workbook(&b)
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    let context = EvaluationContext::default();
    for (cell, expected) in [
        ("A2", CellValue::Number(4.0)),
        ("E1", CellValue::Text("abc".into())),
        ("G2", CellValue::Boolean(false)),
        ("J2", CellValue::Number(2.5)),
        ("N1", CellValue::Text("B".into())),
        ("P2", CellValue::Text("b/b".into())),
        ("T2", CellValue::Text("b2".into())),
    ] {
        assert_eq!(direct.cell(at(cell)).unwrap(), expected, "direct {cell}");
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at(cell))
                .unwrap(),
            expected,
            "overlay {cell}"
        );
    }
}

#[test]
fn textjoin_cycles_the_complete_delimiter_sequence_between_accepted_texts() {
    // Independently authored values exercise the repeating range-delimiter rule
    // in Microsoft TEXTJOIN example 3, retrieved 2026-10-25:
    // https://support.microsoft.com/en-gb/excel/functions/textjoin-function
    // The source's city/address fixture data is not reused.
    for (expression, expected) in [
        (
            r#"TEXTJOIN({",",";"},TRUE,{"a","b";"c","d";"e","f"})"#,
            "a,b;c,d;e,f",
        ),
        (r#"TEXTJOIN({"|","/"},TRUE,{"","a","","b","c"})"#, "a|b/c"),
        (r#"TEXTJOIN("|",FALSE,{"","a",""})"#, "|a|"),
    ] {
        assert_eq!(
            formula(expression),
            CellValue::Text(expected.into()),
            "{expression}"
        );
    }
}
