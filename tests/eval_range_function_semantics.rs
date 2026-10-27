//! Expectations use the ordered SUMIF/SUMPRODUCT definitions documented by Microsoft:
//! https://support.microsoft.com/en-us/office/sumif-function-169b8c99-c05c-4483-a712-1697a653039b
//! https://support.microsoft.com/en-us/office/sumproduct-function-16753e75-9f68-4874-94ac-4d2145a2fd2e
//! Exact ordering, lazy evaluation, dependency invalidation and observer behavior are Poirot contracts.
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::eval::*;
use poirot::model::{CellError, CellValue, Workbook};

/// Address one public workbook target.
fn at(sheet: usize, cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet,
        address: cell.parse().unwrap(),
    }
}
/// Create two sheets for cross-sheet and same-sheet controls.
fn book() -> Workbook {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    book.add_sheet("Other").unwrap();
    book
}
/// Set an independently specified input value.
fn set(book: &mut Workbook, sheet: usize, cell: &str, value: impl Into<CellValue>) {
    book.sheet_mut(sheet)
        .unwrap()
        .set_value(at(sheet, cell).address, value)
        .unwrap();
}
/// Set a public formula source.
fn formula(book: &mut Workbook, sheet: usize, cell: &str, source: &str) {
    book.sheet_mut(sheet)
        .unwrap()
        .set_formula(at(sheet, cell).address, source)
        .unwrap();
}

#[test]
fn repeated_range_calls_return_ordered_results_in_both_per_cell_modes() {
    for (n, q) in [(8, 16), (32, 8), (64, 64)] {
        for family in ["SUMIF", "SUMPRODUCT"] {
            let mut book = book();
            for row in 1..=n {
                set(&mut book, 0, &format!("A{row}"), row as f64);
                set(&mut book, 0, &format!("B{row}"), 2.);
            }
            for row in 1..=q {
                let inner = if family == "SUMIF" {
                    format!("SUMIF(A1:A{n},\">=\"&{},B1:B{n})", n / 2)
                } else {
                    format!("SUMPRODUCT(A1:A{n},B1:B{n})")
                };
                formula(&mut book, 0, &format!("D{row}"), &format!("{inner}+{row}"));
            }
            let expected = if family == "SUMIF" {
                2 * (n - n / 2 + 1)
            } else {
                n * (n + 1)
            };
            let mut direct = Evaluation::with_workbook(&book)
                .build(Default::default())
                .unwrap();
            for row in 1..=q {
                assert_eq!(
                    direct.cell(at(0, &format!("D{row}"))).unwrap(),
                    CellValue::Number((expected + row) as f64)
                );
            }
            let mut overlay = Overlay::with_workbook(&book).build().unwrap();
            let context = EvaluationContext::default();
            let mut run = overlay.evaluation(&context).unwrap();
            for row in 1..=q {
                assert_eq!(
                    run.cell(at(0, &format!("D{row}"))).unwrap(),
                    CellValue::Number((expected + row) as f64)
                );
            }
        }
    }
}

#[test]
fn range_calls_track_transitive_cross_sheet_edits_and_blank_membership() {
    let mut book = book();
    set(&mut book, 1, "F1", 3.);
    formula(&mut book, 1, "A1", "F1*2");
    set(&mut book, 1, "B1", 2.);
    for row in 1..=3 {
        formula(
            &mut book,
            0,
            &format!("D{row}"),
            "SUMPRODUCT(Other!A1:A3,Other!B1:B3)",
        );
    }
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        for row in 1..=3 {
            assert_eq!(run.cell(at(0, &format!("D{row}"))).unwrap(), 12.0.into());
        }
    }
    overlay.set_value(at(1, "F1"), 5.).unwrap();
    assert!(overlay.cached_value(at(0, "D3")).is_none());
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at(0, "D3"))
            .unwrap(),
        20.0.into()
    );
    overlay.set_value(at(1, "A3"), 7.).unwrap();
    overlay.set_value(at(1, "B3"), 3.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at(0, "D2"))
            .unwrap(),
        41.0.into()
    );
    let snapshot = overlay.snapshot().into_owned().unwrap();
    let (restored_book, state) = Overlay::from_snapshot(snapshot).unwrap();
    let mut restored = state.attach(&restored_book).unwrap();
    restored.set_value(at(1, "B3"), 4.).unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at(0, "D2"))
            .unwrap(),
        48.0.into()
    );
}

#[test]
fn sumif_resized_target_partial_ranges_parameters_axes_and_formula_edits() {
    let mut book = book();
    for (cell, value) in [
        ("A1", 1.),
        ("A2", 2.),
        ("A3", 3.),
        ("B1", 10.),
        ("B2", 20.),
        ("B3", 30.),
    ] {
        set(&mut book, 0, cell, value);
    }
    for (cell, source) in [
        ("D1", "SUMIF(A1:A3,\">1\",B1)"),
        ("D2", "SUMIF(A1:A3,\">1\",B1)"),
        ("D6", "SUMIF(A1:A3,\">1\",B1)"),
        ("D3", "SUMIF(A1:A2,\">1\",B1)"),
        ("D4", "SUMIF(A1:A3,\">2\",B1)"),
        ("D5", "SUMIF(A1:C1,\">1\",A2:C2)"),
    ] {
        formula(&mut book, 0, cell, source);
    }
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        for (cell, value) in [
            ("D1", 50.),
            ("D2", 50.),
            ("D6", 50.),
            ("D3", 20.),
            ("D4", 30.),
            ("D5", 20.),
        ] {
            assert_eq!(run.cell(at(0, cell)).unwrap(), value.into());
        }
    }
    overlay.set_value(at(0, "B3"), 40.).unwrap();
    assert!(overlay.cached_value(at(0, "D6")).is_none());
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at(0, "D2"))
            .unwrap(),
        60.0.into()
    );
    overlay
        .set_formula(at(0, "D2"), "SUMIF(A1:A3,\">2\",B1)")
        .unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at(0, "D2"))
            .unwrap(),
        40.0.into()
    );
}

#[test]
fn range_calls_preserve_ordered_cancellation_and_reference_coercion() {
    let mut book = book();
    for (cell, value) in [
        ("A1", CellValue::Number(1e16)),
        ("A2", 1.0.into()),
        ("A3", (-1e16).into()),
        ("A4", false.into()),
        ("A5", "7".into()),
        ("A6", CellValue::Blank),
    ] {
        set(&mut book, 0, cell, value);
    }
    for row in 1..=6 {
        set(&mut book, 0, &format!("B{row}"), 1.);
    }
    for (cell, source) in [
        ("D1", "SUMPRODUCT(A1:A6,B1:B6)"),
        ("D2", "SUMPRODUCT(A1:A6,B1:B6)"),
        ("D5", "SUMPRODUCT(A1:A6,B1:B6)"),
        ("D3", "SUMIF(B1:B6,1,A1:A6)"),
        ("D4", "SUMIF(B1:B6,1,A1:A6)"),
        ("D6", "SUMIF(B1:B6,1,A1:A6)"),
    ] {
        formula(&mut book, 0, cell, source);
    }
    let mut run = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    // Ordered additions cancel the rounded-away one; reference FALSE, text and blank contribute zero.
    for cell in ["D1", "D2", "D5", "D3", "D4", "D6"] {
        assert_eq!(run.cell(at(0, cell)).unwrap(), 0.0.into());
    }
}

#[test]
fn unselected_range_errors_remain_lazy() {
    let mut book = book();
    set(&mut book, 0, "A1", 0.);
    set(&mut book, 0, "A2", 1.);
    set(&mut book, 0, "B1", CellValue::Error(CellError::NA));
    set(&mut book, 0, "B2", 9.);
    for (cell, source) in [
        ("D1", "SUMIF(A1:A2,1,B1:B2)"),
        ("D2", "SUMIF(A1:A2,1,B1:B2)"),
        ("D3", "SUMIF(A1:A2,0,B1:B2)"),
        ("D4", "SUMIF(A1:A2,0,B1:B2)"),
    ] {
        formula(&mut book, 0, cell, source);
    }
    let mut run = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    for cell in ["D1", "D2"] {
        assert_eq!(run.cell(at(0, cell)).unwrap(), 9.0.into());
    }
    for cell in ["D3", "D4"] {
        assert_eq!(
            run.cell(at(0, cell)).unwrap(),
            CellValue::Error(CellError::NA)
        );
    }
}

#[test]
fn mixed_callers_do_not_inherit_prior_local_reads_or_context() {
    for q in [8, 32, 128] {
        let mut book = book();
        for row in 1..=16 {
            set(&mut book, 0, &format!("A{row}"), 1.);
        }
        for row in 1..=q {
            set(&mut book, 0, &format!("E{row}"), row as f64);
            formula(
                &mut book,
                0,
                &format!("D{row}"),
                &format!("E{row}+SUMIF(A1:A16,1)"),
            );
        }
        let mut overlay = Overlay::with_workbook(&book).build().unwrap();
        let context = EvaluationContext::default();
        {
            let mut run = overlay.evaluation(&context).unwrap();
            for row in 1..=q {
                assert_eq!(
                    run.cell(at(0, &format!("D{row}"))).unwrap(),
                    (16. + row as f64).into()
                );
            }
        }
        overlay.set_value(at(0, "E1"), 100.).unwrap();
        assert!(overlay.cached_value(at(0, "D1")).is_none());
        for row in 2..=q {
            assert!(overlay.cached_value(at(0, &format!("D{row}"))).is_some());
        }
        let mut run = overlay.evaluation(&context).unwrap();
        for row in 1..=q {
            run.cell(at(0, &format!("D{row}"))).unwrap();
        }
        assert_eq!(run.usage().formula_executions, 1, "q = {q}");
    }
}

#[test]
fn generated_children_keep_outside_owner_edges_through_growth_and_shrink() {
    let mut book = book();
    set(&mut book, 0, "F1", 3.);
    set(&mut book, 0, "G1", 1.);
    formula(&mut book, 0, "A1", "SEQUENCE(F1,2,G1)");
    formula(&mut book, 0, "D1", "SUMPRODUCT(B1:B3,B1:B3)");
    formula(&mut book, 0, "D2", "SUMPRODUCT(B1:B3,B1:B3)+1");
    formula(&mut book, 0, "D3", "SUMPRODUCT(B1:B3,B1:B3)+2");
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at(0, "D1")).unwrap(), 56.0.into());
        assert_eq!(run.cell(at(0, "D2")).unwrap(), 57.0.into());
        assert_eq!(run.cell(at(0, "D3")).unwrap(), 58.0.into());
    }
    for (cell, value, expected) in [("G1", 2., 84.), ("F1", 2., 35.), ("F1", 4., 84.)] {
        overlay.set_value(at(0, cell), value).unwrap();
        assert!(overlay.cached_value(at(0, "D2")).is_none());
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at(0, "D2"))
                .unwrap(),
            expected.into()
        );
    }
}

#[test]
fn prior_clock_reads_do_not_spread_and_scalar_volatile_arguments_still_execute() {
    let mut book = book();
    set(&mut book, 0, "A1", 1.);
    set(&mut book, 0, "A2", 1.);
    formula(&mut book, 0, "D1", "NOW()+SUMIF(A1:A2,1)");
    formula(&mut book, 0, "D2", "SUMIF(A1:A2,1)");
    formula(&mut book, 0, "D4", "SUMIF(A1:A2,1)");
    // Both RAND calls precede the same final draw in these two equivalent formulas.
    formula(
        &mut book,
        0,
        "D3",
        "SUMIF(A1:A2,IF(RAND()<1,1,2))+SUMIF(A1:A2,IF(RAND()<1,1,2))+RAND()",
    );
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let mut context = EvaluationContext {
        as_of_time: std::time::UNIX_EPOCH,
        ..Default::default()
    };
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at(0, "D1")).unwrap(), 25571.0.into());
        assert_eq!(run.cell(at(0, "D2")).unwrap(), 2.0.into());
        assert_eq!(run.cell(at(0, "D4")).unwrap(), 2.0.into());
    }
    context.as_of_time += std::time::Duration::from_secs(86400);
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at(0, "D1"))
            .unwrap(),
        25572.0.into()
    );
    assert!(overlay.cached_value(at(0, "D2")).is_some());
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    let random_result = direct.cell(at(0, "D3")).unwrap();
    direct
        .set_formula(at(0, "D3"), "4+IF(RAND()<1,0,0)+IF(RAND()<1,0,0)+RAND()")
        .unwrap();
    assert_eq!(direct.cell(at(0, "D3")).unwrap(), random_result);
}

/// Collect actual public read events to verify observer mode retains ordinary execution.
#[derive(Default)]
struct Reads(Vec<ReadEvent>);
impl DependencyReadSink for Reads {
    fn read(&mut self, event: ReadEvent) {
        self.0.push(event);
    }
}

#[test]
fn external_observers_receive_reads_and_arrays_retain_their_channel() {
    let mut book = book();
    for (cell, value) in [("A1", 1.), ("A2", 2.), ("B1", 3.), ("B2", 4.)] {
        set(&mut book, 0, cell, value);
    }
    formula(&mut book, 0, "D1", "SUMPRODUCT(A1:A2,B1:B2)");
    formula(&mut book, 0, "D2", "SUMPRODUCT(A1:A2,B1:B2)");
    formula(&mut book, 0, "D5", "SUMPRODUCT(A1:A2,B1:B2)");
    formula(&mut book, 0, "D3", "SUMPRODUCT({1;2},{3;4})");
    formula(&mut book, 0, "D4", "SUMPRODUCT({1;2},{3;4})");
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
    let view = WorkbookView::new(&book, &plan, &policy).unwrap();
    let mut reads = Reads::default();
    let mut run = EvaluationRun::new(view, Default::default(), Default::default())
        .unwrap()
        .with_dependency_sink(&mut reads);
    for cell in ["D1", "D2", "D5", "D3", "D4"] {
        assert_eq!(run.evaluate_cell(at(0, cell)).unwrap(), 11.0.into());
    }
    drop(run);
    for consumer in [at(0, "D1"), at(0, "D2")] {
        assert!(reads.0.iter().any(|event| event.consumer == Some(consumer)
            && event.read == DependencyRead::Cell(at(0, "B2"))));
    }
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    for cell in ["D3", "D4"] {
        assert_eq!(direct.cell(at(0, cell)).unwrap(), 11.0.into());
    }
    let mut trace = observe::BoundedExecutionTrace::new(observe::RetentionLimits {
        events: 1000,
        scalar_values: 1000,
        text_bytes: 10000,
    });
    let view = WorkbookView::new(&book, &plan, &policy).unwrap();
    let mut run = EvaluationRun::new(view, Default::default(), Default::default())
        .unwrap()
        .with_execution_observer(&mut trace);
    for cell in ["D1", "D2", "D5"] {
        assert_eq!(run.evaluate_cell(at(0, cell)).unwrap(), 11.0.into());
    }
    drop(run);
    assert!(!trace.events().is_empty());
    let registry = functions::extensions::ExtensionRegistry::new();
    let mut overlay = Overlay::with_workbook(&book)
        .with_extensions(&registry)
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let mut run = overlay.evaluation(&context).unwrap();
    for cell in ["D1", "D2", "D5"] {
        assert_eq!(run.cell(at(0, cell)).unwrap(), 11.0.into());
    }
}

#[test]
fn admission_refusal_and_aborts_leave_no_partial_publication_or_reusable_run() {
    let mut book = book();
    set(&mut book, 0, "A1", 1.);
    set(&mut book, 0, "A2", 2.);
    formula(&mut book, 0, "D1", "SUMIF(A1:A2,\">0\")");
    formula(&mut book, 0, "D2", "SUMIF(A1:A2,\">0\")");
    formula(&mut book, 0, "E1", "REPT(\"x\",100)");
    let mut context = EvaluationContext::default();
    context.limits = context.limits.max_collection_values(8);
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    for cell in ["D1", "D2"] {
        assert_eq!(direct.cell(at(0, cell)).unwrap(), 3.0.into());
    }
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    context.limits = poirot::eval::EvaluationLimits::default().max_text_bytes(50);
    {
        let mut run = overlay.evaluation(&context).unwrap();
        let failure = run
            .outputs(&[at(0, "D1"), at(0, "D2"), at(0, "E1")])
            .unwrap_err();
        assert_eq!(run.cell(at(0, "D2")).unwrap_err(), failure);
    }
    assert!(overlay.cached_value(at(0, "D1")).is_none());
    assert!(overlay.cached_value(at(0, "D2")).is_none());
    let mut run = overlay.evaluation(&context).unwrap();
    assert_eq!(run.cell(at(0, "D2")).unwrap(), 3.0.into());
}

#[test]
fn an_operational_failure_inside_a_range_function_cannot_publish_or_resume() {
    let mut book = book();
    for row in 1..=64 {
        set(&mut book, 0, &format!("A{row}"), 1.);
    }
    set(&mut book, 0, "E1", 3.);
    set(&mut book, 0, "E2", 4.);
    formula(&mut book, 0, "D1", "E1+SUMIF(A1:A64,1)");
    formula(&mut book, 0, "D2", "E2+SUMIF(A1:A64,1)");
    let mut calibration = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(calibration.cell(at(0, "D1")).unwrap(), 67.0.into());
    let work = match calibration.finish().outcome {
        EvaluationOutcome::Complete { usage, .. } => usage.work_units,
        other => panic!("{other:?}"),
    };
    let mut context = EvaluationContext::default();
    context.limits = context.limits.max_work(work + work / 2);
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at(0, "D1")).unwrap(), 67.0.into());
        let failure = run.cell(at(0, "D2")).unwrap_err();
        assert_eq!(run.cell(at(0, "D1")).unwrap_err(), failure);
    }
    assert!(overlay.cached_value(at(0, "D1")).is_some());
    assert!(overlay.cached_value(at(0, "D2")).is_none());
    context.limits = poirot::eval::EvaluationLimits::default();
    let mut run = overlay.evaluation(&context).unwrap();
    assert_eq!(run.cell(at(0, "D2")).unwrap(), 68.0.into());
}
