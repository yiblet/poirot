//! Public modern defaults and the shared boundaries repaired by the regression audit.
use poirot_eval::overlay::{EvaluationContext, Overlay, OverlayPolicy};
use poirot_eval::*;
use poirot_model::{CellError, CellValue, ExcelText, Workbook};

fn target(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
fn book() -> Workbook {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    book
}
fn complete(outcome: EvaluationOutcome<CellValue>) -> CellValue {
    match outcome {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}

#[test]
fn defaults_are_consistent_and_serialized_policy_ordinals_are_preserved() {
    let configuration = RunConfiguration::default();
    let overlay = OverlayPolicy::default();
    assert_eq!(
        configuration.builtin_policy(),
        BuiltinSemanticsPolicy::ExcelDocumentation
    );
    assert_eq!(
        configuration.text_policy().source,
        TextSourceVersion::ExcelCompatibilityV2Documented
    );
    assert_eq!(
        configuration.array_result_policy(),
        ArrayResultPolicy::DynamicSpill
    );
    assert_eq!(configuration.builtin_policy(), overlay.builtin);
    assert_eq!(configuration.text_policy(), overlay.text);
    assert_eq!(configuration.array_result_policy(), overlay.arrays);
    assert_eq!(
        postcard::to_allocvec(&BuiltinSemanticsPolicy::PinnedPoi).unwrap(),
        [0]
    );
    assert_eq!(
        postcard::to_allocvec(&BuiltinSemanticsPolicy::ExcelDocumentation).unwrap(),
        [1]
    );
    assert_eq!(
        postcard::to_allocvec(&ArrayResultPolicy::ScalarIntersection).unwrap(),
        [0]
    );
    assert_eq!(
        postcard::to_allocvec(&ArrayResultPolicy::DynamicSpill).unwrap(),
        [1]
    );
}

#[test]
fn text_and_unary_regressions_work_in_prepared_bound_and_overlay_paths() {
    // SEARCH/LEFT Version 2 count surrogate pairs; wildcard and argument results
    // also retain the independently authored ClosedXML corpus expectations.
    // https://support.microsoft.com/en-us/excel/functions/search-function
    // https://support.microsoft.com/en-gb/excel/compatibility-versions
    // Unary plus leaves its value unchanged (MS-XLSB PtgUPlus):
    // https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/381dd053-7283-4a70-98ea-49ddcc9abfed
    let text = |value: &str| CellValue::Text(value.into());
    let cases = [
        (r#"SEARCH("a~?","a?")"#, CellValue::Number(1.)),
        (r#"SEARCH("~*","ab*cd")"#, CellValue::Number(3.)),
        (r#"SEARCH("~~","ab~cd")"#, CellValue::Number(3.)),
        (r#"SEARCH("~","ab~cd")"#, CellValue::Number(3.)),
        (r#"SEARCH("b*","ab")"#, CellValue::Number(2.)),
        (
            r#"SEARCH("soft*2010","Microsoft Excel 2010")"#,
            CellValue::Number(6.),
        ),
        (r#"SEARCH("?Z","😀Z")"#, CellValue::Number(1.)),
        (r#"SEARCH("z","İz")"#, CellValue::Number(2.)),
        (r#"SEARCH("z","ßz")"#, CellValue::Number(2.)),
        (r#"SEARCH("a","aba",2)"#, CellValue::Number(3.)),
        (r#"SEARCH("x","abc")"#, CellValue::Error(CellError::Value)),
        (r#"FIND("Z","😀Z")"#, CellValue::Number(2.)),
        (r#"LEFT("👩Z",1)"#, text("👩")),
        (r#"RIGHT("Z👩",1)"#, text("👩")),
        (r#"LEFT("edge",)"#, text("")),
        (r#"RIGHT("edge",)"#, text("")),
        (r#"LEFT("edge")"#, text("e")),
        (r#"CONCAT(,A1,"","x")"#, text("x")),
        (
            r#"MID("ABC",2147483648,1)"#,
            CellValue::Error(CellError::Value),
        ),
        (
            r#"MID("ABC",1,2147483648)"#,
            CellValue::Error(CellError::Value),
        ),
        (r#"REPT("",2147483648)"#, CellValue::Error(CellError::Value)),
        (r#"LOWER("ͶꝎＫǢ")"#, text("ͷꝏｋǣ")),
        (r#"PROPER("hÉLLO wÖRLD")"#, text("Héllo Wörld")),
        ("ISBLANK(+A1)", CellValue::Boolean(true)),
        ("ISLOGICAL(+TRUE)", CellValue::Boolean(true)),
        (r#"ISTEXT(+"1")"#, CellValue::Boolean(true)),
        ("ISREF(+A1)", CellValue::Boolean(false)),
        (r#"CODE("A")"#, CellValue::Number(65.)),
        // ClosedXML's pinned Excel edge observations and independent Calc agree:
        // normalize clamped components before validating the resulting date.
        ("DATE(10000,-32000,1)", CellValue::Number(1984455.)),
    ];
    let mut book = book();
    for (i, (formula, _)) in cases.iter().enumerate() {
        book.sheet_mut(0)
            .unwrap()
            .set_formula(
                poirot_model::CellAddress {
                    row: i as u32,
                    col: 2,
                },
                *formula,
                CellValue::Blank,
            )
            .unwrap();
    }
    let policy = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&book, &policy);
    let mut run = EvaluationRun::new(
        WorkbookView::new(&book, &plan, &policy).unwrap(),
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    for (i, (formula, expected)) in cases.iter().enumerate() {
        let at = EvaluationTarget {
            sheet: 0,
            address: poirot_model::CellAddress {
                row: i as u32,
                col: 2,
            },
        };
        let prepared = compiler.compile_formula(formula, &policy);
        let bound =
            bound::FormulaPlan::bind(&poirot_parsing::formula::parse_formula(formula).unwrap())
                .unwrap();
        assert_eq!(
            complete(run_formula(
                &book,
                0,
                at.address,
                formula,
                Default::default(),
                Default::default()
            )),
            *expected,
            "direct {formula}"
        );
        assert_eq!(
            run.evaluate_formula(at, &prepared).unwrap(),
            *expected,
            "prepared {formula}"
        );
        assert_eq!(
            run.evaluate_bound(at, &bound).unwrap(),
            *expected,
            "bound {formula}"
        );
        assert_eq!(
            overlay.evaluation(&context).unwrap().cell(at).unwrap(),
            *expected,
            "overlay {formula}"
        );
    }
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (restored, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut overlay = state.attach(&restored).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C1"))
            .unwrap(),
        CellValue::Number(1.)
    );
}

#[test]
fn logical_units_preserve_unpaired_surrogates_and_policy_changes_invalidate_snapshots() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_value(
            target("A1").address,
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800, 0xd83d, 0xde00, 0xdc00])),
        )
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(target("B1").address, "LEFT(A1,2)", CellValue::Blank)
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(target("B2").address, "LEN(A1)", CellValue::Blank)
        .unwrap();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B1"))
            .unwrap()
            .text_ref()
            .unwrap()
            .units()
            .collect::<Vec<_>>(),
        vec![0xd800, 0xd83d, 0xde00]
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B2"))
            .unwrap(),
        CellValue::Number(3.)
    );
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (book, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut overlay = state.attach(&book).unwrap();
    let mut policy = overlay.policy().clone();
    policy.text.source = TextSourceVersion::Poi7321209;
    overlay.set_policy(policy).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B2"))
            .unwrap(),
        CellValue::Number(4.)
    );
    overlay.set_value(target("A1"), "😀").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B2"))
            .unwrap(),
        CellValue::Number(2.)
    );
}

#[test]
fn default_recalculation_atomically_publishes_and_replaces_spill_children() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(target("A1").address, "{1,2;3,4}", CellValue::Blank)
        .unwrap();
    assert!(matches!(
        run_recalculation(&mut book, Default::default(), Default::default()),
        EvaluationOutcome::Complete { value: 1, .. }
    ));
    assert_eq!(
        book.sheet(0)
            .unwrap()
            .get(target("B2").address)
            .unwrap()
            .value,
        CellValue::Number(4.)
    );
    assert_eq!(
        book.sheet(0).unwrap().spill_owner(target("B2").address),
        Some(target("A1").address)
    );
    let bytes = postcard::to_allocvec(&poirot_model::snapshot::Snapshot::capture(&book)).unwrap();
    let snapshot: poirot_model::snapshot::Snapshot<'_> = postcard::from_bytes(&bytes).unwrap();
    let mut book = snapshot.restore().unwrap();
    let before = postcard::to_allocvec(&poirot_model::snapshot::Snapshot::capture(&book)).unwrap();
    assert!(matches!(
        run_recalculation_with_receipts(
            &mut book,
            &RunConfiguration::default(),
            EvaluationBudgets {
                run_work: 1,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
    assert_eq!(
        postcard::to_allocvec(&poirot_model::snapshot::Snapshot::capture(&book)).unwrap(),
        before
    );
    book.sheet_mut(0)
        .unwrap()
        .set_formula(target("A1").address, "{9}", CellValue::Blank)
        .unwrap();
    assert!(matches!(
        run_recalculation_with_receipts(
            &mut book,
            &RunConfiguration::default(),
            Default::default()
        ),
        EvaluationOutcome::Complete { .. }
    ));
    assert!(book.sheet(0).unwrap().get(target("B2").address).is_none());
    assert_eq!(
        book.sheet(0)
            .unwrap()
            .get(target("A1").address)
            .unwrap()
            .value,
        CellValue::Number(9.)
    );
}
