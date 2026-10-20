use poirot_eval::*;
use poirot_model::{CellValue, RowVisibility, Workbook};
fn target(at: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: at.parse().unwrap(),
    }
}
fn excel() -> RunConfiguration {
    RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation)
}
fn workbook() -> Workbook {
    let mut w = Workbook::new();
    let s = w.add_sheet("S").unwrap();
    for (at, n) in [("A1", 2.), ("A2", 3.), ("A3", 5.)] {
        s.set_value(target(at).address, n).unwrap();
    }
    s.set_formula(target("B1").address, "SUBTOTAL(9,A1:A3)")
        .unwrap();
    w
}
fn formula(w: &Workbook, f: &str) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        w,
        0,
        target("Z1").address,
        f,
        &excel(),
        EvaluationBudgets::default(),
    )
}
fn value(w: &Workbook, f: &str) -> CellValue {
    match formula(w, f) {
        EvaluationOutcome::Complete { value, .. } => value,
        result => panic!("{result:?}"),
    }
}
#[test]
fn visibility_flags_are_independent_and_filtered_errors_never_evaluate() {
    let mut w = workbook();
    let s = w.sheet_mut(0).unwrap();
    s.set_row_visibility(
        1,
        RowVisibility {
            manually_hidden: true,
            filtered_out: false,
        },
    )
    .unwrap();
    s.set_row_visibility(
        2,
        RowVisibility {
            manually_hidden: false,
            filtered_out: true,
        },
    )
    .unwrap();
    s.set_formula(target("A3").address, "1/0").unwrap();
    assert_eq!(value(&w, "SUBTOTAL(9,A1:A3)"), CellValue::Number(5.));
    assert_eq!(value(&w, "SUBTOTAL(109,A1:A3)"), CellValue::Number(2.));
    w.sheet_mut(0)
        .unwrap()
        .set_row_visibility(2, RowVisibility::default())
        .unwrap();
    assert_eq!(
        value(&w, "SUBTOTAL(109,A1:A3)"),
        CellValue::Error(poirot_model::CellError::Div0)
    );
    assert!(matches!(
        run_formula_with_configuration(
            &w,
            0,
            target("Z1").address,
            "SUBTOTAL(9,A1:A3)",
            &RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi),
            Default::default()
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::UnsupportedCapability { .. },
            ..
        }
    ));
}
#[derive(Default)]
/// Records subtotal metadata reads separately from evaluated cell values.
struct Reads(Vec<ReadEvent>);
impl DependencyReadSink for Reads {
    fn read(&mut self, event: ReadEvent) {
        self.0.push(event)
    }
}
#[test]
fn nested_detection_is_syntax_based_and_exclusion_preserves_metadata_reads() {
    let mut w = workbook();
    w.sheet_mut(0)
        .unwrap()
        .set_formula(target("A4").address, "IF(TRUE,1/0,SUBTOTAL(9,A1:A3))")
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_formula(target("A5").address, "\"SUBTOTAL(9,A1:A3)\"")
        .unwrap();
    let p = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&w, &p);
    let f = compiler.compile_formula("SUBTOTAL(3,A1:A5)", &p);
    let c = excel();
    let mut reads = Reads::default();
    let mut run = EvaluationRun::new_with_configuration(
        WorkbookView::new(&w, &plan, &p).unwrap(),
        &c,
        Default::default(),
    )
    .unwrap()
    .with_dependency_sink(&mut reads);
    assert_eq!(
        run.evaluate_formula(target("Z1"), &f).unwrap(),
        CellValue::Number(4.)
    );
    let result = run.finish(Ok(()));
    assert_eq!(result.execution.formula_executions, 2); // Probe and ordinary text formula; excluded A4 is not executed.
    assert!(
        reads
            .0
            .iter()
            .any(|r| r.read == DependencyRead::SubtotalMetadata(target("A4")))
    );
    assert!(
        !reads
            .0
            .iter()
            .any(|r| r.read == DependencyRead::Cell(target("A4")))
    );
    assert_eq!(value(&w, "SUBTOTAL(9,(A1:A3,B1))"), CellValue::Number(10.));
}
#[test]
fn visibility_changes_invalidate_plans_and_nested_overlay_edits_invalidate_results() {
    use poirot_eval::overlay::{EvaluationContext, Overlay, OverlayPolicy};
    let mut w = workbook();
    let p = EvaluationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&w, &p);
    w.sheet_mut(0)
        .unwrap()
        .set_row_visibility(
            0,
            RowVisibility {
                filtered_out: true,
                manually_hidden: false,
            },
        )
        .unwrap();
    assert!(matches!(
        WorkbookView::new(&w, &plan, &p),
        Err(EvaluationFailure::StalePlan)
    ));
    let mut overlay = Overlay::with_workbook(&w)
        .with_policy(OverlayPolicy {
            builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let b = target("B1");
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(b).unwrap(),
        CellValue::Number(8.)
    );
    let mut evaluation = overlay.evaluation(&context).unwrap();
    evaluation.cell(b).unwrap();
    assert_eq!(evaluation.usage().formula_executions, 0);
    drop(evaluation);
    overlay.set_formula(target("A2"), "SUBTOTAL(9,A3)").unwrap();
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(b).unwrap(),
        CellValue::Number(5.)
    );
    overlay.set_value(target("A2"), 7.).unwrap();
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(b).unwrap(),
        CellValue::Number(12.)
    );
}
#[test]
fn reference_materialization_and_metadata_text_reads_remain_budgeted() {
    let mut w = workbook();
    let budgets = EvaluationBudgets {
        materialized_values: 2,
        ..Default::default()
    };
    assert!(matches!(
        run_formula_with_configuration(
            &w,
            0,
            target("Z1").address,
            "SUBTOTAL(9,A1:A3)",
            &excel(),
            budgets
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
    w.sheet_mut(0)
        .unwrap()
        .set_formula(
            target("A4").address,
            format!("SUBTOTAL(9,A1)+{}1", " ".repeat(100)),
        )
        .unwrap();
    let budgets = EvaluationBudgets {
        text_bytes: 50,
        ..Default::default()
    };
    assert!(matches!(
        run_formula_with_configuration(
            &w,
            0,
            target("Z1").address,
            "SUBTOTAL(9,A4)",
            &excel(),
            budgets
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
}
#[test]
fn shared_run_overlay_replaces_nested_formula_but_keeps_row_visibility() {
    let mut w = workbook();
    let s = w.sheet_mut(0).unwrap();
    s.set_formula(target("A2").address, "SUBTOTAL(9,A3)")
        .unwrap();
    s.set_row_visibility(
        2,
        RowVisibility {
            filtered_out: true,
            manually_hidden: false,
        },
    )
    .unwrap();
    let p = EvaluationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&w, &p);
    let shared =
        poirot_eval::run::shared_workbook::SharedWorkbook::new(w, std::sync::Arc::new(plan))
            .unwrap();
    let (_, inputs) = shared
        .prepare_inputs::<_, poirot_model::ModelError>(|_| {
            Ok((
                (),
                vec![(0, target("A2").address, CellValue::Number(7.))],
                1,
            ))
        })
        .unwrap();
    let r = shared
        .run_workbook(&excel(), Default::default(), &inputs, |run, _| {
            run.evaluate_cell(target("B1"))
        })
        .unwrap();
    assert!(matches!(
        r.report.outcome,
        EvaluationOutcome::Complete {
            value: CellValue::Number(9.),
            ..
        }
    ));
}
