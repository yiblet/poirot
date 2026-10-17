//! Unary plus preserves selected values; it does not forward a reference channel.
use poirot_eval::overlay::{EvaluationContext, Overlay, OverlayPolicy};
use poirot_eval::*;
use poirot_model::{CellError, CellValue, ExcelText, Workbook};

fn target(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}
fn workbook() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value(target("A2").address, "1").unwrap();
    sheet.set_value(target("A3").address, true).unwrap();
    sheet.set_value(target("A4").address, 2.).unwrap();
    sheet
        .set_value(
            target("A5").address,
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800, 0x61, 0xdc00])),
        )
        .unwrap();
    sheet
        .set_formula(target("B4").address, "+A2:A4", CellValue::Blank)
        .unwrap();
    book
}

#[test]
fn unary_plus_preserves_selected_value_types() {
    // Scalar expectations are independently corroborated by Calc. Array member
    // preservation and loss of reference identity are explicit Poirot contracts;
    // Calc instead preserves +reference identity and coerces mixed array literals.
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        let book = workbook();
        let policy = EvaluationPolicy::default();
        let configuration = RunConfiguration::default().with_array_result_policy(arrays);
        let mut compiler = FormulaCompiler::new();
        let plan = compiler.compile_workbook(&book, &policy);
        let mut run = EvaluationRun::new_with_configuration(
            WorkbookView::new(&book, &plan, &policy).unwrap(),
            &configuration,
            Default::default(),
        )
        .unwrap();
        let text = |s: &str| CellValue::Text(s.into());
        for (formula, expected) in [
            ("+0", CellValue::Number(0.)),
            ("+42", CellValue::Number(42.)),
            ("+\"1\"", text("1")),
            ("+\"pear\"", text("pear")),
            ("+\"\"", text("")),
            ("+TRUE", CellValue::Boolean(true)),
            ("+FALSE", CellValue::Boolean(false)),
            ("+#DIV/0!", CellValue::Error(CellError::Div0)),
            ("+#N/A", CellValue::Error(CellError::NA)),
            ("+A1", CellValue::Blank),
            (
                "+A5",
                CellValue::TextUtf16(ExcelText::from_units(vec![0xd800, 0x61, 0xdc00])),
            ),
            ("ISBLANK(+A1)", CellValue::Boolean(true)),
            ("ISBLANK(+IF(TRUE,))", CellValue::Boolean(true)),
            ("ISNUMBER(+\"1\")", CellValue::Boolean(false)),
            ("ISTEXT(+\"😀\")", CellValue::Boolean(true)),
            ("ISLOGICAL(+A3)", CellValue::Boolean(true)),
            ("ISREF(+A2)", CellValue::Boolean(false)),
            ("ISREF(+IF(TRUE,A2))", CellValue::Boolean(false)),
            ("ISREF(IF(TRUE,A2))", CellValue::Boolean(true)),
            ("+IF(TRUE,A2,1/0)", text("1")),
            ("-\"1\"", CellValue::Number(-1.)),
            ("-TRUE", CellValue::Number(-1.)),
            ("-\"pear\"", CellValue::Error(CellError::Value)),
            ("-A1", CellValue::Number(0.)),
            ("+\"1\"+2", CellValue::Number(3.)),
            ("SUM(+{\"1\",TRUE,2})", CellValue::Number(2.)),
            // SUMPRODUCT admits the preserved boolean as one and ignores text.
            ("SUMPRODUCT(+A2:A4)", CellValue::Number(3.)),
            (
                "ISTEXT(INDEX(+{\"1\",TRUE,2},1,1))",
                CellValue::Boolean(true),
            ),
            (
                "ISLOGICAL(INDEX(+{\"1\",TRUE,2},1,2))",
                CellValue::Boolean(true),
            ),
            ("SUM(+{1,#N/A})", CellValue::Error(CellError::NA)),
        ] {
            let prepared = compiler.compile_formula(formula, &policy);
            let bound =
                bound::FormulaPlan::bind(&poirot_formula::parse_formula(formula).unwrap()).unwrap();
            assert_eq!(
                run.evaluate_formula(target("F2"), &prepared).unwrap(),
                expected,
                "{arrays:?} {formula}"
            );
            assert_eq!(
                run.evaluate_bound(target("F2"), &bound).unwrap(),
                expected,
                "bound {arrays:?} {formula}"
            );
        }
        // A collection caller must not alter the worksheet formula's own demand,
        // including when that formula's scalar result was already memoized.
        let selected = if arrays == ArrayResultPolicy::ScalarIntersection {
            CellValue::Number(2.)
        } else {
            text("1")
        };
        assert_eq!(run.evaluate_cell(target("B4")).unwrap(), selected);
        let formula = compiler.compile_formula("SUMPRODUCT(+B4*{1})", &policy);
        assert_eq!(
            run.evaluate_formula(target("F2"), &formula).unwrap(),
            CellValue::Number(if arrays == ArrayResultPolicy::ScalarIntersection {
                2.
            } else {
                1.
            })
        );
    }
}

#[test]
fn pinned_numeric_policy_retains_its_unary_plus_source_contract() {
    let book = workbook();
    let configuration =
        RunConfiguration::default().with_numeric_policy(NumericOutcomePolicy::PinnedPoi7321209);
    for (formula, expected) in [
        ("+TRUE", CellValue::Number(1.)),
        ("+FALSE", CellValue::Number(0.)),
        ("+A1", CellValue::Number(0.)),
        ("+\"1\"", CellValue::Text("1".into())),
    ] {
        assert!(
            matches!(run_formula_with_configuration(&book, 0, target("F2").address, formula, &configuration, Default::default()), EvaluationOutcome::Complete { value, .. } if value == expected),
            "{formula}"
        );
    }
}

#[test]
fn unary_plus_reads_invalidate_after_typed_edits_and_snapshot_restore() {
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        let mut book = workbook();
        book.sheet_mut(0)
            .unwrap()
            .set_formula(target("C2").address, "ISTEXT(+A2)", CellValue::Blank)
            .unwrap();
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(OverlayPolicy {
                arrays,
                ..Default::default()
            })
            .build()
            .unwrap();
        let context = EvaluationContext::default();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("C2"))
                .unwrap(),
            CellValue::Boolean(true)
        );
        overlay.set_value(target("A2"), true).unwrap();
        assert!(overlay.cached_value(target("C2")).is_none());
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("C2"))
                .unwrap(),
            CellValue::Boolean(false)
        );
        let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
        let (restored_book, restored_state) =
            Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
        let mut overlay = restored_state.attach(&restored_book).unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("C2"))
                .unwrap(),
            CellValue::Boolean(false)
        );
        overlay.remove_cell(target("A2")).unwrap();
        assert!(overlay.cached_value(target("C2")).is_none());
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("C2"))
                .unwrap(),
            CellValue::Boolean(false)
        );
    }
}

#[derive(Default)]
struct Reads(Vec<ReadEvent>);
impl DependencyReadSink for Reads {
    fn read(&mut self, event: ReadEvent) {
        self.0.push(event);
    }
}

#[test]
fn unary_plus_array_admission_precedes_reads_and_failures_remain_operational() {
    let book = workbook();
    let policy = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&book, &policy);
    let formula = compiler.compile_formula("SUMPRODUCT(+A2:A4)", &policy);
    let mut reads = Reads::default();
    let mut run = EvaluationRun::new(
        WorkbookView::new(&book, &plan, &policy).unwrap(),
        Default::default(),
        EvaluationBudgets {
            materialized_values: 2,
            ..Default::default()
        },
    )
    .unwrap()
    .with_dependency_sink(&mut reads);
    assert!(matches!(
        run.evaluate_formula(target("F2"), &formula),
        Err(EvaluationFailure::Budget(_))
    ));
    run.finish(Ok(()));
    assert!(
        !reads
            .0
            .iter()
            .any(|event| matches!(event.read, DependencyRead::Cell(_)))
    );
}
