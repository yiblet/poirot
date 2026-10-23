//! Independent contracts from Microsoft's REGEXTEST/REGEXEXTRACT/REGEXREPLACE
//! function pages (https://support.microsoft.com/en-us/excel/functions/regexextract-function).
//! Examples and expected strings were authored here; no upstream fixtures reused.
use poirot_eval::{
    overlay::{Evaluation, EvaluationContext, Overlay, OverlayPolicy},
    *,
};
use poirot_model::{CellError, CellValue, ExcelText, Workbook};
fn at(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}
fn book() -> Workbook {
    let mut b = Workbook::new();
    b.add_sheet("S").unwrap();
    b
}
fn formula(source: &str) -> CellValue {
    match run_formula(
        &book(),
        0,
        at("Z100").address,
        source,
        Default::default(),
        Default::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{source}: {other:?}"),
    }
}
#[test]
fn matching_uses_pcre2_lookarounds_backreferences_and_case_options() {
    for (f, expected) in [
        (r#"REGEXTEST("id:42;","(?<=id:)\d+(?=;)")"#, true),
        (r#"REGEXTEST("elm elm","\b(\w+)\s+\1\b")"#, true),
        (r#"REGEXTEST("elm oak","\b(\w+)\s+\1\b")"#, false),
        (r#"REGEXTEST("Ä","ä")"#, false),
        (r#"REGEXTEST("Ä","ä",1)"#, true),
        (r#"REGEXTEST(812,"^8")"#, true),
        (r#"REGEXTEST(TRUE,"^TRUE$")"#, true),
        (r#"REGEXTEST(A1,"^$")"#, true),
        (r#"REGEXTEST("x","x",)"#, true),
        (r#"REGEXTEST("x","x",A1)"#, true),
    ] {
        assert_eq!(formula(f), CellValue::Boolean(expected), "{f}");
    }
}
#[test]
fn extraction_and_replacement_preserve_text_capture_and_occurrence_contracts() {
    for (f, expected) in [
        (r#"REGEXEXTRACT("x=12 y=34","\d+")"#, "12"),
        (r#"INDEX(REGEXEXTRACT("x=12 y=34","\d+",1),2,1)"#, "34"),
        (
            r#"INDEX(REGEXEXTRACT("oak-12 elm-34","([a-z]+)-(\d+)",2),1,2)"#,
            "12",
        ),
        (
            r#"REGEXREPLACE("a1 b2 c3","([a-z])(\d)","$2:$1")"#,
            "1:a 2:b 3:c",
        ),
        (
            r#"REGEXREPLACE("a1 b2 c3","([a-z])(\d)","$2:$1",2)"#,
            "a1 2:b c3",
        ),
        (
            r#"REGEXREPLACE("a1 b2 c3","([a-z])(\d)","$2:$1",-1)"#,
            "a1 b2 3:c",
        ),
        (
            r#"REGEXREPLACE("a1 b2 c3","([a-z])(\d)","$2:$1",-2)"#,
            "a1 2:b c3",
        ),
        (r#"REGEXREPLACE("a1","z","x")"#, "a1"),
        (r#"REGEXREPLACE("AA","a","x",,1)"#, "xx"),
        (r#"REGEXREPLACE("é😀","","|")"#, "|é|😀|"),
        (r#"REGEXREPLACE("a","(?:|a)","x")"#, "xxx"),
        (r#"REGEXEXTRACT("8","\d",,0)"#, "8"),
    ] {
        assert_eq!(formula(f), CellValue::Text(expected.into()), "{f}");
    }
}
#[test]
fn domain_errors_and_element_errors_remain_spreadsheet_values() {
    for (f, expected) in [
        (r#"REGEXTEST("x","[")"#, CellError::Value),
        (r#"REGEXTEST("x","x",2)"#, CellError::Value),
        (r#"REGEXEXTRACT("x","x",3)"#, CellError::Value),
        (r#"REGEXEXTRACT("x","z")"#, CellError::NA),
        (r#"REGEXTEST(#DIV/0!,"x")"#, CellError::Div0),
        (r#"REGEXREPLACE("x",#REF!,"y")"#, CellError::Ref),
        (r#"REGEXREPLACE("x","x",#N/A)"#, CellError::NA),
        (r#"REGEXTEST()"#, CellError::Value),
        (r#"REGEXEXTRACT("a")"#, CellError::Value),
        (r#"REGEXREPLACE("a","a")"#, CellError::Value),
        (
            r#"INDEX(REGEXTEST({"a",#DIV/0!},"a"),1,2)"#,
            CellError::Div0,
        ),
    ] {
        assert_eq!(formula(f), CellValue::Error(expected), "{f}");
    }
}
#[test]
fn scalar_functions_broadcast_arrays_and_reference_ranges() {
    assert_eq!(
        formula(r#"INDEX(REGEXTEST({"a1";"b2"},{"^a","2$"}),2,2)"#),
        CellValue::Boolean(true)
    );
    assert_eq!(
        formula(r#"INDEX(REGEXEXTRACT({"a1";"b2"},"\d"),2,1)"#),
        CellValue::Text("2".into())
    );
    assert_eq!(
        formula(r#"INDEX(REGEXREPLACE({"a1";"b2"},"\d","x"),2,1)"#),
        CellValue::Text("bx".into())
    );
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("J1").address, "a1").unwrap();
    s.set_value(at("J2").address, "b2").unwrap();
    s.set_formula(at("A1").address, r#"REGEXTEST(J1:J2,"^a")"#)
        .unwrap();
    let mut direct = Evaluation::with_workbook(&b)
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(direct.cell(at("A2")).unwrap(), CellValue::Boolean(false));
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("A2"))
            .unwrap(),
        CellValue::Boolean(false)
    );
    overlay.set_value(at("J2"), "a2").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("A2"))
            .unwrap(),
        CellValue::Boolean(true)
    );
}
#[test]
fn extraction_spills_child_first_obstructions_and_overlay_shape_changes() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("J1").address, "oak-12 elm-34 ash-56")
        .unwrap();
    s.set_formula(at("A1").address, r#"_xlfn.REGEXEXTRACT(J1,"\d+",1)"#)
        .unwrap();
    s.set_formula(at("D1").address, r#"REGEXEXTRACT(J1,"([a-z]+)-(\d+)",2)"#)
        .unwrap();
    s.set_formula(at("G1").address, r#"REGEXEXTRACT(J1,"\d+",1)"#)
        .unwrap();
    s.set_value(at("G2").address, 7.).unwrap();
    let mut direct = Evaluation::with_workbook(&b)
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    let context = EvaluationContext::default();
    for (cell, value) in [
        ("A3", CellValue::Text("56".into())),
        ("E1", CellValue::Text("12".into())),
        ("G1", CellValue::Error(CellError::Spill)),
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
    overlay.set_value(at("J1"), "fir-8").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("A3"))
            .unwrap(),
        CellValue::Blank
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("A1"))
            .unwrap(),
        CellValue::Text("8".into())
    );
    overlay.set_value(at("J1"), "fir-8 pine-9").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("A2"))
            .unwrap(),
        CellValue::Text("9".into())
    );
}
#[test]
fn native_exhaustion_and_unsupported_unicode_cannot_be_caught_by_iferror() {
    for f in [
        r#"IFERROR(REGEXTEST("aaaaaaaaaaaaaaaaaaaa!","(*LIMIT_MATCH=10)(*NO_START_OPT)(a+)+$"),99)"#,
        r#"IFERROR(REGEXEXTRACT("aaaaaaaaaaaaaaaaaaaa!","(*LIMIT_MATCH=10)(*NO_START_OPT)(a+)+$",1),99)"#,
        r#"IFERROR(REGEXREPLACE("aaaaaaaaaaaaaaaaaaaa!","(*LIMIT_MATCH=10)(*NO_START_OPT)(a+)+$","x"),99)"#,
        r#"IFERROR(REGEXTEST("é","^\C$"),99)"#,
    ] {
        assert!(
            matches!(
                run_formula(
                    &book(),
                    0,
                    at("Z100").address,
                    f,
                    Default::default(),
                    Default::default()
                ),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::UnsupportedCapability { .. }
                        | EvaluationFailure::Resource {
                            kind: ResourceFailureKind::BackendGuard,
                            ..
                        },
                    ..
                }
            ),
            "{f}"
        );
    }
    let mut b = book();
    b.sheet_mut(0)
        .unwrap()
        .set_value(
            at("A1").address,
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800])),
        )
        .unwrap();
    assert!(matches!(
        run_formula(
            &b,
            0,
            at("Z100").address,
            r#"IFERROR(REGEXTEST(A1,"."),99)"#,
            Default::default(),
            Default::default()
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::UnsupportedCapability { .. },
            ..
        }
    ));
}
#[test]
fn explicit_evaluation_budgets_remain_operational() {
    for (formula, budgets) in [
        (
            r#"IFERROR(REGEXEXTRACT("a b c d","\w",1),99)"#,
            EvaluationBudgets {
                materialized_values: 3,
                ..Default::default()
            },
        ),
        (
            r#"IFERROR(REGEXREPLACE("a","a","aaaa"),99)"#,
            EvaluationBudgets {
                text_bytes: 2,
                ..Default::default()
            },
        ),
        (
            r#"IFERROR(REGEXTEST("abc","."),99)"#,
            EvaluationBudgets {
                cell_work: 2,
                ..Default::default()
            },
        ),
    ] {
        assert!(
            matches!(
                run_formula_with_configuration(
                    &book(),
                    0,
                    at("Z100").address,
                    formula,
                    &RunConfiguration::default(),
                    budgets
                ),
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
fn pinned_poi_profile_does_not_claim_modern_regex_support() {
    for f in [
        r#"REGEXTEST("x","x")"#,
        r#"REGEXEXTRACT("x","x")"#,
        r#"REGEXREPLACE("x","x","y")"#,
    ] {
        let mut b = book();
        b.sheet_mut(0)
            .unwrap()
            .set_formula(at("A1").address, f)
            .unwrap();
        let mut direct = Evaluation::with_workbook(&b)
            .with_policy(OverlayPolicy {
                builtin: BuiltinSemanticsPolicy::PinnedPoi,
                ..Default::default()
            })
            .build(Default::default())
            .unwrap();
        assert_eq!(
            direct.cell(at("A1")).unwrap(),
            CellValue::Error(CellError::Name)
        );
    }
}
#[test]
fn catalog_preserves_typed_results_and_possible_numeric_coercion_effects() {
    use poirot_eval::registry::{DependencyPolicy, ResultRule, descriptors_for_policy};
    use poirot_model::{FormulaType, ScalarKinds};
    let scalar = FormulaType::scalar(ScalarKinds::TEXT);
    for (name, rule) in [
        ("REGEXTEST", ResultRule::ElementwiseBoolean),
        ("REGEXREPLACE", ResultRule::ElementwiseText),
        ("REGEXEXTRACT", ResultRule::RegexExtract),
    ] {
        let descriptor = descriptors_for_policy(BuiltinSemanticsPolicy::ExcelDocumentation)
            .find(|d| d.name == name)
            .unwrap();
        assert_eq!(descriptor.result, rule);
        assert!(
            descriptor
                .dependencies
                .contains(DependencyPolicy::NUMERIC_COERCION)
        );
        assert!(!descriptor.dependencies.contains(DependencyPolicy::SEED));
        let result = rule.apply(&[scalar]);
        assert!(!ScalarKinds::NUMBER.is_subtype_of(result.elements()));
        assert_eq!(result.may_spill(), name == "REGEXEXTRACT");
        assert!(
            rule.apply(&[FormulaType::array(ScalarKinds::TEXT)])
                .may_spill()
        );
    }
}
