use poirot_eval::{
    CapabilityId, EvaluationBudgets, EvaluationFailure, EvaluationInputs, EvaluationOutcome,
    bound::{Expression, FormulaPlan},
    run_formula,
};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};
fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Caller").unwrap();
    workbook.add_sheet("O'Brien").unwrap();
    workbook.define_name("Rate", "10", None).unwrap();
    workbook.define_name("Rate", "20", Some(0)).unwrap();
    workbook.define_name("Rate", "30", Some(1)).unwrap();
    workbook.define_name("GlobalOnly", "40", None).unwrap();
    workbook.define_name("CallerCell", "A1", Some(1)).unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(CellAddress { row: 0, col: 0 }, 5.)
        .unwrap();
    workbook
        .sheet_mut(1)
        .unwrap()
        .set_value(CellAddress { row: 0, col: 0 }, 9.)
        .unwrap();
    workbook
}
#[test]
fn explicit_scope_selects_name_but_body_retains_caller_context() {
    let workbook = workbook();
    let at = CellAddress { row: 0, col: 2 };
    for (formula, expected) in [
        ("Rate", 20.),
        ("'O''Brien'!Rate", 30.),
        ("'O''Brien'!GlobalOnly", 40.),
        ("'O''Brien'!CallerCell", 5.),
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &workbook,
                0,
                at,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Number(expected),
            "{formula}"
        );
    }
    let syntax = poirot_formula::parse_formula("'O''Brien'!Rate").unwrap();
    assert!(
        matches!(FormulaPlan::bind(&syntax).unwrap().expression(), Expression::QualifiedName {sheet, name} if sheet == "O'Brien" && name == "Rate")
    );
}
#[test]
fn missing_scope_is_deferred_and_never_uses_caller_scope() {
    let workbook = workbook();
    let at = CellAddress { row: 0, col: 2 };
    assert_eq!(
        match poirot_eval::run_formula(
            &workbook,
            0,
            at,
            "MissingSheet!Rate",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Ref)
    );
    assert_eq!(
        match poirot_eval::run_formula(
            &workbook,
            0,
            at,
            "'O''Brien'!MissingName",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Name)
    );
    assert_eq!(
        match poirot_eval::run_formula(
            &workbook,
            0,
            at,
            "IF(FALSE,MissingSheet!Rate,7)",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(7.)
    );
}

#[test]
fn cycles_use_selected_record_identity_across_qualifiers() {
    let mut workbook = workbook();
    workbook
        .define_name("Loop", "'O''Brien'!Loop", None)
        .unwrap();
    let at = CellAddress { row: 0, col: 2 };
    assert_eq!(
        match poirot_eval::run_formula(
            &workbook,
            0,
            at,
            "Caller!Loop",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Ref)
    );
}

#[test]
fn qualified_reference_names_keep_caller_cells_and_reference_identity() {
    let mut workbook = workbook();
    workbook
        .define_name("CallerArea", "A1:A2", Some(1))
        .unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(CellAddress { row: 1, col: 0 }, 7.)
        .unwrap();
    workbook
        .sheet_mut(1)
        .unwrap()
        .set_value(CellAddress { row: 1, col: 0 }, 100.)
        .unwrap();
    let at = CellAddress { row: 0, col: 2 };
    for (formula, expected) in [
        ("SUM('O''Brien'!CallerArea)", 12.),
        ("ROW('O''Brien'!CallerCell)", 1.),
        ("COLUMN('O''Brien'!CallerCell)", 1.),
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &workbook,
                0,
                at,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Number(expected),
            "{formula}"
        );
    }
}

#[test]
fn qualified_identifiers_preserve_checked_comparison_failures() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Caller").unwrap();
    workbook.import_sheet("Σ").unwrap();
    workbook.define_name("Rate", "7", Some(1)).unwrap();
    workbook.define_name("ΣRate", "9", Some(0)).unwrap();
    let at = CellAddress { row: 0, col: 2 };
    for formula in ["'σ'!Rate", "Caller!σRate"] {
        assert!(
            matches!(
                run_formula(
                    &workbook,
                    0,
                    at,
                    formula,
                    EvaluationInputs::default(),
                    EvaluationBudgets::default()
                ),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::UnsupportedCapability {
                        capability: CapabilityId::IdentifierComparison,
                        ..
                    },
                    ..
                }
            ),
            "{formula}"
        );
    }
    assert_eq!(
        match poirot_eval::run_formula(
            &workbook,
            0,
            at,
            "'Σ'!Rate",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(7.)
    );
    assert_eq!(
        match poirot_eval::run_formula(
            &workbook,
            0,
            at,
            "IF(FALSE,'σ'!Rate,8)",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(8.)
    );
}

#[test]
fn qualified_latin1_names_choose_local_scope_using_checked_policy() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Caller").unwrap();
    workbook.add_sheet("Ä Data").unwrap();
    workbook.define_name("ÉRate", "10", None).unwrap();
    workbook.define_name("ÉRate", "30", Some(1)).unwrap();
    assert_eq!(
        match poirot_eval::run_formula(
            &workbook,
            0,
            CellAddress { row: 0, col: 0 },
            "'ä dATA'!éRATE",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(30.)
    );
}

#[test]
fn prepared_qualified_names_retain_typed_potential_and_consumed_scope_queries() {
    use poirot_eval::compiler::PreparedFormula;
    use poirot_eval::run::{
        DependencyRead, DependencyReadSink, EvaluationRun, ReadEvent, WorkbookView,
    };
    use poirot_eval::{EvaluationPolicy, EvaluationTarget, FormulaCompiler};
    #[derive(Default)]
    /// Records actual scoped-name lookups for prepared qualified-name tests.
    struct Reads(Vec<DependencyRead>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event.read);
        }
    }
    let workbook = workbook();
    let policy = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    let target = EvaluationTarget {
        sheet: 0,
        address: CellAddress { row: 0, col: 2 },
    };
    for (formula, expected, expected_reads) in [
        (
            "'O''Brien'!Rate",
            CellValue::Number(30.),
            vec![
                DependencyRead::Sheet {
                    current: 0,
                    name: "O'Brien".into(),
                    selected: Some(1),
                },
                DependencyRead::Name {
                    sheet: 1,
                    name: "Rate".into(),
                    selected: Some(2),
                },
            ],
        ),
        (
            "'O''Brien'!MissingName",
            CellValue::Error(CellError::Name),
            vec![
                DependencyRead::Sheet {
                    current: 0,
                    name: "O'Brien".into(),
                    selected: Some(1),
                },
                DependencyRead::Name {
                    sheet: 1,
                    name: "MissingName".into(),
                    selected: None,
                },
            ],
        ),
        (
            "IF(FALSE,MissingSheet!Rate,7)",
            CellValue::Number(7.),
            // The skipped name has no reads; IF still selects profile-dependent semantics.
            vec![DependencyRead::BuiltinPolicy],
        ),
    ] {
        let prepared = compiler.compile_formula(formula, &policy);
        let PreparedFormula::Ready(compiled) = prepared.as_ref() else {
            panic!("{formula}");
        };
        let potential = compiled.potential_dependencies();
        assert_eq!(potential.qualified_names.len(), 1);
        assert_eq!(
            potential.qualified_names[0].sheet,
            if formula.starts_with("IF(") {
                "MissingSheet"
            } else {
                "O'Brien"
            }
        );
        assert_eq!(
            potential.qualified_names[0].name,
            if formula.contains("MissingName") {
                "MissingName"
            } else {
                "Rate"
            }
        );
        assert!(potential.names.is_empty());
        assert!(potential.references.is_empty());
        let mut reads = Reads::default();
        let mut run = EvaluationRun::new(
            WorkbookView::new(&workbook, &plan, &policy).unwrap(),
            EvaluationInputs::default(),
            EvaluationBudgets::default(),
        )
        .unwrap()
        .with_dependency_sink(&mut reads);
        assert_eq!(
            run.evaluate_formula(target, &prepared).unwrap(),
            expected,
            "{formula}"
        );
        drop(run);
        assert_eq!(reads.0, expected_reads, "{formula}");
    }
}
