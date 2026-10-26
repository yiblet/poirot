//! Independent examples from Microsoft's GROUPBY and PERCENTOF contracts and arithmetic.
//! https://support.microsoft.com/en-us/excel/functions/groupby-function
//! https://support.microsoft.com/en-us/excel/functions/percentof-function
use poirot::eval::{
    overlay::{Evaluation, EvaluationContext, Overlay},
    *,
};
use poirot::model::{CellError, CellValue, Workbook};
fn target(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}
fn result(book: &Workbook, formula: &str) -> CellValue {
    match run_formula_with_configuration(
        book,
        0,
        target("Z100").address,
        formula,
        &RunConfiguration::default(),
        EvaluationBudgets::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{formula}: {other:?}"),
    }
}
fn formula(formula: &str) -> CellValue {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    result(&book, formula)
}
#[test]
fn unequal_groups_recompute_totals_and_multiple_reductions() {
    for (formula_text, expected) in [
        ("INDEX(GROUPBY({2;1;1},{20;2;8},AVERAGE,0,1),3,2)", 10.0),
        ("INDEX(GROUPBY({2;1;1},{20;2;8},SUM,0,1),1,2)", 10.0),
        (
            "INDEX(GROUPBY({2;1;1},{20;2;8},PERCENTOF,0,1),1,2)",
            1.0 / 3.0,
        ),
        ("INDEX(GROUPBY({2;1;1},{20;2;8},PERCENTOF,0,1),3,2)", 1.0),
        ("INDEX(GROUPBY({1;1;2},{-8;-3;-5},MAX,0,0),1,2)", -3.0),
        ("INDEX(GROUPBY({1;1;2},{8;3;5},MIN,0,0),1,2)", 3.0),
        (
            "INDEX(GROUPBY({1;1;2},{2,20;8,80;6,60},HSTACK(SUM,AVERAGE),0,0),1,5)",
            50.0,
        ),
        (
            "INDEX(GROUPBY({1;1;2},{2;8;6},VSTACK(SUM,AVERAGE),0,0),2,3)",
            5.0,
        ),
        ("PERCENTOF({2;3},{2;3;5})", 0.5),
        ("INDEX(GROUPBY({1;1;1},{2;8;5},MEDIAN,0,0),1,2)", 5.0),
        ("INDEX(GROUPBY({1;1},{2;4},VAR.P,0,0),1,2)", 1.0),
        ("INDEX(GROUPBY({1;1},{2;4},STDEV.S,0,0),1,2)", 2.0f64.sqrt()),
    ] {
        assert_eq!(
            formula(formula_text),
            CellValue::Number(expected),
            "{formula_text}"
        );
    }
}
#[test]
fn counts_headers_filters_sorting_and_lazy_branches() {
    for (formula_text, expected) in [
        (
            r#"INDEX(GROUPBY({1;1;1},{2;"x";#N/A},COUNT,0,0),1,2)"#,
            1.0.into(),
        ),
        (
            r#"INDEX(GROUPBY({1;1;1},{2;"x";#N/A},COUNTA,0,0),1,2)"#,
            3.0.into(),
        ),
        (
            r#"INDEX(GROUPBY({"Key";1;2},{"Value";5;8},SUM,3,0),2,2)"#,
            5.0.into(),
        ),
        (
            r#"INDEX(GROUPBY({"Key";1;2},{"Value";5;8},SUM,1,0),1,2)"#,
            5.0.into(),
        ),
        ("INDEX(GROUPBY({1;2;3},{2;8;5},SUM,0,0,-2),1,1)", 2.0.into()),
        (
            "INDEX(GROUPBY({1;1;2},{#N/A;3;7},SUM,0,0,,{FALSE;TRUE;TRUE}),1,2)",
            3.0.into(),
        ),
        ("IF(FALSE,GROUPBY(1,2,MissingAggregator),17)", 17.0.into()),
        ("IFERROR(GROUPBY(1,2,\"SUM\"),19)", 19.0.into()),
        ("PERCENTOF(2,0)", CellValue::Error(CellError::Div0)),
    ] {
        assert_eq!(formula(formula_text), expected, "{formula_text}");
    }
}
#[test]
fn hierarchy_table_and_source_order_are_distinct() {
    let base = r#"GROUPBY({2,2;1,2;2,1;1,1},{"a";"b";"c";"d"},CONCAT,0,2)"#;
    assert_eq!(
        formula(&format!("INDEX({base},7,3)")),
        CellValue::Text("abcd".into())
    );
    assert_eq!(
        formula(&format!("INDEX({base},3,3)")),
        CellValue::Text("bd".into())
    );
    for (call, row, expected) in [
        ("GROUPBY({2,1;1,2;1,1},{5;8;2},SUM,0,0,{2;1},,0)", 1, 1.0),
        ("GROUPBY({2,1;1,2;1,1},{5;8;2},SUM,0,0,{2;1},,1)", 2, 2.0),
        ("GROUPBY({2,1;1,2;1,1},{5;8;2},SUM,0,-2)", 1, 15.0),
    ] {
        let column = if call.contains(",-2)") { 3 } else { 1 };
        assert_eq!(
            formula(&format!("INDEX({call},{row},{column})")),
            CellValue::Number(expected)
        );
    }
}
#[test]
fn invalid_geometry_controls_errors_and_arity() {
    for call in [
        "GROUPBY({1;2},{3;4;5},SUM)",
        "GROUPBY(1,2,SUM,4)",
        "GROUPBY(1,2,SUM,0,2)",
        "GROUPBY(1,2,SUM,0,0,0)",
        "GROUPBY({1;2},{3;4},SUM,0,0,,{TRUE,FALSE})",
        "GROUPBY({1,2;3,4},{5;6},SUM,0,2,,,1)",
        "GROUPBY(1,2)",
        "PERCENTOF(1)",
        "GROUPBY(1,2,SUM,0,0,,,,3)",
    ] {
        assert_eq!(formula(call), CellValue::Error(CellError::Value), "{call}");
    }
    assert_eq!(
        formula("GROUPBY(1,2,SUM,0,0,,FALSE)"),
        CellValue::Error(CellError::Calc)
    );
    assert_eq!(
        formula("INDEX(GROUPBY({1;1},{3;#N/A},SUM,0,0),1,2)"),
        CellValue::Error(CellError::NA)
    );
}
#[test]
fn scoped_aliases_cycles_and_conflicting_values() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    book.add_sheet("Other").unwrap();
    book.define_name("Reducer", "SUM", None).unwrap();
    book.define_name("Reducer", "AVERAGE", Some(0)).unwrap();
    book.define_name("Pair", "HSTACK(SUM,AVERAGE)", None)
        .unwrap();
    book.define_name("Loop", "Loop", None).unwrap();
    book.define_name("Scalar", "4", None).unwrap();
    assert_eq!(
        result(&book, "INDEX(GROUPBY({1;1},{2;8},Reducer,0,0),1,2)"),
        5.0.into()
    );
    assert_eq!(
        result(&book, "INDEX(GROUPBY({1;1},{2;8},Other!Reducer,0,0),1,2)"),
        10.0.into()
    );
    assert_eq!(
        result(&book, "INDEX(GROUPBY({1;1},{2;8},Pair,0,0),1,3)"),
        5.0.into()
    );
    assert_eq!(
        result(&book, "GROUPBY(1,2,Loop)"),
        CellValue::Error(CellError::Ref)
    );
    assert_eq!(
        result(&book, "GROUPBY(1,2,Scalar)"),
        CellValue::Error(CellError::Value)
    );
    assert_eq!(result(&book, "SUM"), CellValue::Error(CellError::Name));
}
#[test]
fn direct_overlay_edits_and_spill_children() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, value) in [
        ("A1", 1.0),
        ("A2", 1.0),
        ("A3", 2.0),
        ("B1", 2.0),
        ("B2", 8.0),
        ("B3", 6.0),
        ("C1", 1.0),
        ("C2", 1.0),
        ("C3", 1.0),
    ] {
        sheet.set_value(cell.parse().unwrap(), value).unwrap();
    }
    sheet
        .set_formula("E1".parse().unwrap(), "GROUPBY(A1:A3,B1:B3,SUM,0,0,,C1:C3)")
        .unwrap();
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(direct.cell(target("F2")).unwrap(), 6.0.into());
    assert_eq!(direct.cell(target("F1")).unwrap(), 10.0.into());
    drop(direct);
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("F2"))
            .unwrap(),
        6.0.into()
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("F1"))
            .unwrap(),
        10.0.into()
    );
    overlay.set_value(target("B1"), 3.0).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("F1"))
            .unwrap(),
        11.0.into()
    );
    overlay.set_value(target("C2"), 0.0).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("F1"))
            .unwrap(),
        3.0.into()
    );
    overlay.set_value(target("A3"), 1.0).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("F1"))
            .unwrap(),
        9.0.into()
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("F2"))
            .unwrap(),
        CellValue::Blank
    );
}

#[test]
fn a_family_admission_reference_blanks_and_standalone_origin() {
    for (call, expected) in [
        (
            r#"INDEX(GROUPBY({1;1;1},{2;TRUE;"x"},AVERAGEA,0,0),1,2)"#,
            1.0,
        ),
        (r#"INDEX(GROUPBY({1;1;1},{2;TRUE;"x"},MAXA,0,0),1,2)"#, 2.0),
        (r#"INDEX(GROUPBY({1;1;1},{2;TRUE;"x"},MINA,0,0),1,2)"#, 0.0),
        (
            r#"INDEX(GROUPBY({1;1;1},{2;TRUE;"x"},VARPA,0,0),1,2)"#,
            2.0 / 3.0,
        ),
        (r#"PERCENTOF("2",4)"#, 0.5),
        (r#"PERCENTOF({"2";2},4)"#, 0.5),
    ] {
        assert_eq!(formula(call), expected.into(), "{call}");
    }
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for cell in ["A1", "A2", "A3", "A4"] {
        sheet.set_value(cell.parse().unwrap(), 1.0).unwrap();
    }
    sheet.set_value("B1".parse().unwrap(), 2.0).unwrap();
    sheet.set_value("B3".parse().unwrap(), "").unwrap();
    sheet
        .set_value("B4".parse().unwrap(), CellValue::Error(CellError::NA))
        .unwrap();
    assert_eq!(
        result(&book, "INDEX(GROUPBY(A1:A4,B1:B4,COUNT,0,0),1,2)"),
        1.0.into()
    );
    assert_eq!(
        result(&book, "INDEX(GROUPBY(A1:A4,B1:B4,COUNTA,0,0),1,2)"),
        3.0.into()
    );
    assert_eq!(
        result(&book, "INDEX(GROUPBY(A1:A4,B1:B4,COUNTBLANK,0,0),1,2)"),
        2.0.into()
    );
}
#[test]
fn snapshot_preserves_role_programs_and_rejects_a_changed_name_baseline() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_formula(
            "A1".parse().unwrap(),
            "GROUPBY({1;1},{2;8},HSTACK(SUM,AVERAGE),0,0)",
        )
        .unwrap();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C1"))
            .unwrap(),
        5.0.into()
    );
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (book, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach(&book).unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(target("B1"))
            .unwrap(),
        10.0.into()
    );
    restored
        .set_formula(target("A1"), "GROUPBY({1;1},{2;8},HSTACK(AVERAGE,SUM),0,0)")
        .unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(target("B1"))
            .unwrap(),
        5.0.into()
    );
    let mut changed = book.clone();
    changed.define_name("SUM", "4", None).unwrap();
    let (_, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    assert!(state.attach(&changed).is_err());
    assert_eq!(
        result(&changed, "GROUPBY({1;1},{2;8},SUM,0,0)"),
        CellValue::Error(CellError::Value)
    );
}
#[test]
fn independent_program_arguments_use_the_explicit_aggregation_compiler() {
    use poirot::eval::adapters::{FunctionName, ParamVec};
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
    let view = WorkbookView::new(&book, &plan, &policy).unwrap();
    let mut run = EvaluationRun::new(view, Default::default(), Default::default()).unwrap();
    let mut args = Vec::new();
    for (index, source) in ["{1;1}", "{2;8}", "SUM", "0", "0"].into_iter().enumerate() {
        let syntax = poirot_parsing::formula::parse_formula(source).unwrap();
        args.push(if index == 2 {
            bytecode::compile_aggregation_argument(&syntax).unwrap()
        } else {
            bytecode::compile(&syntax).unwrap()
        });
    }
    let arguments: ParamVec<_> = args.into_iter().collect();
    // A grouping call returns its upper-left published member through this scalar API.
    assert_eq!(
        run.call_with_adapter(
            &adapters::builtins(),
            target("Z100"),
            &FunctionName::from("GROUPBY"),
            arguments
        )
        .unwrap(),
        1.0.into()
    );
}
#[test]
fn grouping_budgets_are_operational_and_spill_blockers_are_normal() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    assert!(matches!(
        run_formula_with_configuration(
            &book,
            0,
            target("Z100").address,
            "IFERROR(SUM(GROUPBY(SEQUENCE(1000),SEQUENCE(1000),SUM,0,0)),99)",
            &RunConfiguration::default(),
            EvaluationBudgets {
                materialized_values: 100,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
    let sheet = book.sheet_mut(0).unwrap();
    sheet
        .set_formula("A1".parse().unwrap(), "GROUPBY({1;2},{3;4},SUM,0,0)")
        .unwrap();
    sheet.set_value("B2".parse().unwrap(), 19.0).unwrap();
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(direct.cell(target("B2")).unwrap(), 19.0.into());
    assert_eq!(
        direct.cell(target("A1")).unwrap(),
        CellValue::Error(CellError::Spill)
    );
}

#[test]
fn aggregation_name_misses_are_observed_only_on_reached_calls() {
    /// Actual consumed namespace events, independent of conservative compiler guards.
    #[derive(Default)]
    struct Reads(Vec<DependencyRead>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event.read);
        }
    }
    for (source, expect_name) in [
        ("INDEX(GROUPBY({1;1},{2;8},SUM,0,0),1,2)", true),
        ("IF(FALSE,GROUPBY(1,2,MissingReducer),17)", false),
    ] {
        let mut book = Workbook::new();
        book.add_sheet("S")
            .unwrap()
            .set_formula("A1".parse().unwrap(), source)
            .unwrap();
        let policy = PreparationPolicy::default();
        let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
        let mut reads = Reads::default();
        let view = WorkbookView::new(&book, &plan, &policy).unwrap();
        let mut run = EvaluationRun::new(view, Default::default(), Default::default())
            .unwrap()
            .with_dependency_sink(&mut reads);
        run.evaluate_cell(target("A1")).unwrap();
        drop(run);
        assert_eq!(
            reads.0.iter().any(
                |read| matches!(read,DependencyRead::Name {name,selected:None,..} if name=="SUM")
            ),
            expect_name
        );
        assert!(
            !reads.0.iter().any(
                |read| matches!(read,DependencyRead::Name {name,..} if name=="MissingReducer")
            )
        );
    }
}

#[test]
fn pinned_profile_and_registered_constructor_extensions_remain_separate() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    for source in [
        "GROUPBY(1,2,SUM)",
        "_xlfn.GROUPBY(1,2,SUM)",
        "PERCENTOF(1,2)",
        "_xlfn.PERCENTOF(1,2)",
    ] {
        let outcome = run_formula_with_configuration(
            &book,
            0,
            target("Z100").address,
            source,
            &RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi),
            EvaluationBudgets::default(),
        );
        assert!(
            matches!(
                outcome,
                EvaluationOutcome::Complete {
                    value: CellValue::Error(CellError::Name),
                    ..
                }
            ),
            "{source}"
        );
        assert!(
            !matches!(result(&book, source), CellValue::Error(CellError::Name)),
            "{source}"
        );
    }
    use poirot::eval::extensions::*;
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "HSTACK",
            "group007-test",
            ExtensionContract {
                arity: registry::Arity {
                    minimum: 1,
                    maximum: 255,
                },
                evaluation: ExtensionEvaluationStrategy::Eager,
                capabilities: ExtensionCapabilities {
                    references: true,
                    arrays: true,
                    context: false,
                },
                cacheability: ExtensionCacheability::Deterministic,
            },
            |_host: &mut dyn ExtensionHost| -> ExtensionResult<ExtensionOutput> {
                Ok(ExtensionOutput::Scalar(19.0.into()))
            },
        )
        .unwrap();
    let syntax =
        poirot_parsing::formula::parse_formula("GROUPBY({1;1},{2;8},HSTACK(SUM,AVERAGE),0,0)")
            .unwrap();
    let program = bytecode::compile_with_registry(&syntax, Some(&registry)).unwrap();
    assert!(program.template.aggregations.is_empty());
    let ordinary = poirot_parsing::formula::parse_formula("HSTACK(1)").unwrap();
    assert!(matches!(
        bytecode::compile_with_registry(&ordinary, Some(&registry))
            .unwrap()
            .calls[0]
            .target,
        bytecode::CallTarget::Extension(_)
    ));
}

#[test]
fn generated_headers_obey_utf16_limits_and_intermediate_text_budgets() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1".parse().unwrap(), "Key").unwrap();
    sheet.set_value("A2".parse().unwrap(), 1.0).unwrap();
    sheet
        .set_value("B1".parse().unwrap(), "x".repeat(32767))
        .unwrap();
    sheet.set_value("B2".parse().unwrap(), 3.0).unwrap();
    let call = "GROUPBY(A1:A2,B1:B2,HSTACK(SUM,AVERAGE),3,0)";
    assert_eq!(
        result(&book, &format!("INDEX({call},1,2)")),
        CellValue::Error(CellError::Value)
    );
    book.sheet_mut(0)
        .unwrap()
        .set_value("B1".parse().unwrap(), "x".repeat(32759))
        .unwrap();
    // 32,759 source units plus a space and seven units in AVERAGE exactly fit.
    assert_eq!(
        result(&book, &format!("LEN(INDEX({call},1,3))")),
        32767.0.into()
    );
    assert!(matches!(
        run_formula_with_configuration(
            &book,
            0,
            target("Z100").address,
            "INDEX(GROUPBY({1;2},{3;4},SUM,2,0),2,2)",
            &RunConfiguration::default(),
            EvaluationBudgets {
                text_bytes: 4,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}
