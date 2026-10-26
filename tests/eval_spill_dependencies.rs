//! Spill-sensitive coordinates are incremental nodes with ordinary value cutoffs.
use poirot::eval::{
    overlay::{EvaluationContext, EvaluationPolicy, Overlay},
    *,
};
use poirot::model::{CellValue, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
fn dynamic() -> EvaluationPolicy {
    EvaluationPolicy {
        arrays: ArrayResultPolicy::DynamicSpill,
        ..Default::default()
    }
}

#[test]
fn cached_empty_query_detects_new_anchor_but_unchanged_blank_cuts_off_parent() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula(at("D4").address, "B2+1").unwrap();
    sheet.set_value(at("A2").address, 99.).unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(dynamic())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D4"))
            .unwrap(),
        1.0.into()
    );
    overlay.set_formula(at("A1"), "SEQUENCE(3,2)").unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("D4")).unwrap(), 1.0.into());
    assert_eq!(
        evaluation.usage().formula_executions,
        1,
        "blocked producer runs, parent stays cached"
    );
    drop(evaluation);
    overlay.set_value(at("A2"), CellValue::Blank).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("D4")).unwrap(), 5.0.into());
    assert_eq!(evaluation.usage().formula_executions, 2);
    drop(evaluation);
    overlay.set_formula(at("B2"), "\"\"").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B2"))
            .unwrap(),
        CellValue::from("")
    );
    overlay.set_value(at("B2"), CellValue::Blank).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B2"))
            .unwrap(),
        4.0.into()
    );
}

#[test]
fn unchanged_producer_inputs_restore_whole_array_and_shape_changes_reach_children() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, value) in [("F1", 2.1), ("F2", 10.1)] {
        sheet.set_value(at(cell).address, value).unwrap();
    }
    for (cell, formula) in [
        ("G1", "ROUND(F1,0)"),
        ("G2", "ROUND(F2,0)"),
        ("A1", "SEQUENCE(G1,1,1,G2)"),
        ("D1", "A2*2"),
        ("D2", "SUM(A1#)"),
    ] {
        sheet.set_formula(at(cell).address, formula).unwrap();
    }
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(dynamic())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D1"))
            .unwrap(),
        22.0.into()
    );
    overlay.set_value(at("F1"), 2.2).unwrap();
    overlay.set_value(at("F2"), 10.2).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("D1")).unwrap(), 22.0.into());
    assert_eq!(
        evaluation.usage().formula_executions,
        2,
        "only rounded inputs execute"
    );
    drop(evaluation);
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        12.0.into()
    );
    overlay.set_value(at("F2"), 20.).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("D1")).unwrap(), 42.0.into());
    assert_eq!(evaluation.cell(at("D2")).unwrap(), 22.0.into());
    drop(evaluation);
    overlay.set_value(at("F1"), 3.).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("D1")).unwrap(), 42.0.into());
    assert_eq!(evaluation.cell(at("D2")).unwrap(), 63.0.into());
    assert_eq!(
        evaluation.usage().formula_executions,
        3,
        "unchanged A2 cuts off D1 despite array growth"
    );
    drop(evaluation);
    overlay.set_value(at("F1"), 1.).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("D1")).unwrap(), 0.0.into());
    assert_eq!(evaluation.cell(at("D2")).unwrap(), 1.0.into());
}

#[test]
fn numeric_input_inside_producer_prefix_does_not_change_occupancy() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value(at("A1").address, 1.1).unwrap();
    for (cell, formula) in [
        ("B1", "ROUND(A1,0)"),
        ("D5", "SEQUENCE(B1+1)"),
        ("F8", "D6*2"),
    ] {
        sheet.set_formula(at(cell).address, formula).unwrap();
    }
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(dynamic())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("F8"))
            .unwrap(),
        4.0.into()
    );
    overlay.set_value(at("A1"), 1.2).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("F8")).unwrap(), 4.0.into());
    assert_eq!(evaluation.usage().formula_executions, 1);
}

#[test]
fn empty_query_and_owner_dependencies_survive_snapshots_and_anchor_removal() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula(at("D4").address, "B2+1").unwrap();
    // This unrelated failing producer must never be evaluated by a B2 read.
    sheet
        .set_formula(at("Z99").address, "SEQUENCE(1/0)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(book)
        .with_policy(dynamic())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D4"))
            .unwrap(),
        1.0.into()
    );
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (book, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut overlay = state.attach(book).unwrap();
    overlay.set_formula(at("A1"), "SEQUENCE(3,2)").unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("D4")).unwrap(), 5.0.into());
    assert_eq!(evaluation.usage().formula_executions, 2);
    drop(evaluation);
    assert!(overlay.cached_value(at("Z99")).is_none());
    overlay.remove_cell(at("A1")).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D4"))
            .unwrap(),
        1.0.into()
    );
    overlay.set_formula(at("A1"), "SEQUENCE(3,2)").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B2"))
            .unwrap(),
        4.0.into()
    );
    overlay.set_formula(at("A2"), "\"\"").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D4"))
            .unwrap(),
        1.0.into()
    );
}

#[test]
fn changed_other_array_element_preserves_coordinate_parent_and_failed_batch_publishes_nothing() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value(at("G1").address, 4.).unwrap();
    for (cell, formula) in [
        ("A1", "VSTACK(HSTACK(1,2),HSTACK(3,G1))"),
        ("D1", "A2*10"),
        ("D2", "SUM(A1#)"),
        ("J1", "REPT(\"x\",1000)"),
    ] {
        sheet.set_formula(at(cell).address, formula).unwrap();
    }
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(dynamic())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D1"))
            .unwrap(),
        30.0.into()
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        10.0.into()
    );
    overlay.set_value(at("G1"), 5.).unwrap();
    let mut constrained = context.clone();
    constrained.limits = constrained.limits.max_text_bytes(64);
    let mut failed = overlay.evaluation(&constrained).unwrap();
    assert!(failed.outputs(&[at("D1"), at("J1")]).is_err());
    assert!(failed.result_changes().unwrap().is_empty());
    drop(failed);
    assert!(overlay.cached_value(at("D1")).is_none());
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("D1")).unwrap(), 30.0.into());
    assert_eq!(evaluation.cell(at("D2")).unwrap(), 11.0.into());
    assert_eq!(
        evaluation.usage().formula_executions,
        2,
        "only producer and whole-array sum execute"
    );
}

#[test]
fn warm_coordinate_reconciles_new_competing_top_left_producer_and_removal() {
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula(at("D4").address, "B2+1")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(dynamic())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D4"))
            .unwrap(),
        1.0.into()
    );
    overlay.set_formula(at("A2"), "SEQUENCE(1,3,10)").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D4"))
            .unwrap(),
        12.0.into()
    );
    overlay.set_formula(at("B1"), "SEQUENCE(3,1,20)").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D4"))
            .unwrap(),
        22.0.into()
    );
    overlay.remove_cell(at("B1")).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D4"))
            .unwrap(),
        12.0.into()
    );
}

#[test]
fn clean_unowned_blank_read_needs_only_its_cell_read_budget() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_formula(at("A1").address, "SEQUENCE(100)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(dynamic())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("A99"))
            .unwrap(),
        99.0.into()
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B99"))
            .unwrap(),
        CellValue::Blank
    );
    let parsed = overlay.compiled_usage().parse_attempts;
    let mut constrained = context.clone();
    constrained.limits = constrained.limits.max_work(1);
    let mut evaluation = overlay.evaluation(&constrained).unwrap();
    assert_eq!(evaluation.cell(at("B99")).unwrap(), CellValue::Blank);
    assert_eq!(evaluation.usage().formula_executions, 0);
    drop(evaluation);
    assert_eq!(overlay.compiled_usage().parse_attempts, parsed);
}

#[test]
fn cross_sheet_owner_context_changes_cut_off_unchanged_child_values() {
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula(at("A1").address, "SEQUENCE(2,1,IF(NOW()>0,10,20))")
        .unwrap();
    book.add_sheet("T")
        .unwrap()
        .set_formula(at("D1").address, "S!A2*2")
        .unwrap();
    let output = EvaluationTarget {
        sheet: 1,
        address: at("D1").address,
    };
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(dynamic())
        .build()
        .unwrap();
    let mut context = EvaluationContext::default();
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(output).unwrap(),
        22.0.into()
    );
    let mut warm = overlay.evaluation(&context).unwrap();
    assert_eq!(warm.cell(output).unwrap(), 22.0.into());
    assert_eq!(warm.usage().formula_executions, 0);
    drop(warm);
    context.as_of_time += std::time::Duration::from_secs(1);
    let mut changed = overlay.evaluation(&context).unwrap();
    assert_eq!(changed.cell(output).unwrap(), 22.0.into());
    assert_eq!(
        changed.usage().formula_executions,
        1,
        "time changes rerun producer, unchanged child cuts off cross-sheet parent"
    );
}

#[test]
fn repeated_session_children_keep_priority_spill_references_and_complete_exports() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, formula) in [
        ("A2", "SEQUENCE(1,3,10)"),
        ("B1", "SEQUENCE(3,1,20)"),
        ("D1", "SUM(B1#)"),
        ("D2", "B2+B2"),
    ] {
        sheet.set_formula(at(cell).address, formula).unwrap();
    }
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(dynamic())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    // B1 has top-left priority over the intersecting A2 producer. Its vertical
    // sequence is 20,21,22, giving the sum 63 and the repeated-child sum 42.
    for _ in 0..2 {
        let mut evaluation = overlay.evaluation(&context).unwrap();
        let outputs = evaluation
            .outputs(&[at("B2"), at("B2"), at("D1"), at("D2"), at("B3")])
            .unwrap();
        for (output, expected) in outputs.iter().zip([21., 21., 63., 42., 22.]) {
            assert_eq!(output.value, CellValue::Number(expected));
        }
        assert_eq!(
            evaluation.cell(at("A2")).unwrap(),
            CellValue::Error(poirot::model::CellError::Spill)
        );
        let mut exported = book.clone();
        exported
            .apply(evaluation.result_changes().unwrap())
            .unwrap();
        let sheet = exported.sheet(0).unwrap();
        assert_eq!(
            sheet.complete_spill_extent(at("B1").address),
            Some(at("B3").address)
        );
        for (cell, expected) in [("B1", 20.), ("B2", 21.), ("B3", 22.)] {
            assert_eq!(
                sheet.get(at(cell).address).unwrap().value,
                CellValue::Number(expected)
            );
        }
    }
}

#[test]
fn memoized_producer_has_no_self_ownership_notification() {
    /// Collect public reads, including owner notifications.
    struct Reads(std::rc::Rc<std::cell::RefCell<Vec<ReadEvent>>>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.borrow_mut().push(event);
        }
    }
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula(at("A1").address, "SEQUENCE(2)")
        .unwrap();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
    let configuration =
        RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::DynamicSpill);
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut reads = Reads(events.clone());
    let mut run = EvaluationRun::new_with_configuration(
        WorkbookView::new(&book, &plan, &policy).unwrap(),
        &configuration,
        Default::default(),
    )
    .unwrap()
    .with_dependency_sink(&mut reads);
    assert_eq!(run.evaluate_cell(at("A1")).unwrap(), 1.0.into());
    events.borrow_mut().clear();
    assert_eq!(run.evaluate_cell(at("A1")).unwrap(), 1.0.into());
    assert_eq!(
        *events.borrow(),
        [ReadEvent {
            consumer: None,
            read: DependencyRead::Cell(at("A1"))
        }],
        "producer reads do not report the producer as an additional owner"
    );
}
