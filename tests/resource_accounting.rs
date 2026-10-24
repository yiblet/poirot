//! Public resource contracts for bounded scalars and sparse reference consumers.
use poirot::eval::{
    self, EvaluationBudgets, EvaluationFailure, EvaluationInputs, EvaluationOutcome,
    EvaluationTarget,
};
use poirot::{CellValue, EvaluationContext, Workbook, eval::overlay::Overlay};

fn target(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
fn run(book: &Workbook, formula: &str, budgets: EvaluationBudgets) -> EvaluationOutcome<CellValue> {
    eval::run_formula(
        book.model(),
        0,
        target("Z1").address,
        formula,
        EvaluationInputs::default(),
        budgets,
    )
}
fn complete(outcome: EvaluationOutcome<CellValue>) -> CellValue {
    match outcome {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
fn book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    sheet.set_value("A1".parse().unwrap(), "first").unwrap();
    sheet.set_value("A1000".parse().unwrap(), "last").unwrap();
    sheet.set_value("B1".parse().unwrap(), 7.).unwrap();
    sheet.set_value("B1000".parse().unwrap(), 11.).unwrap();
    book
}
#[test]
fn sparse_whole_columns_preserve_order_and_real_work_boundaries() {
    let book = book();
    for (formula, expected) in [
        ("CONCAT(A:A)", CellValue::Text("firstlast".into())),
        (
            "TEXTJOIN(\".\",TRUE,A:A)",
            CellValue::Text("first.last".into()),
        ),
        ("MAXIFS(B:B,A:A,\"last\")", CellValue::Number(11.)),
        ("COUNTIF(A:A,\"last\")", CellValue::Number(1.)),
    ] {
        let outcome = run(&book, formula, EvaluationBudgets::default());
        let work = outcome.usage().work_units;
        assert_eq!(complete(outcome), expected, "{formula}");
        assert!(work < 1000, "{formula}: {work}");
        assert_eq!(
            complete(run(
                &book,
                formula,
                EvaluationBudgets {
                    run_work: work,
                    ..Default::default()
                }
            )),
            expected
        );
        assert!(matches!(
            run(
                &book,
                formula,
                EvaluationBudgets {
                    run_work: work - 1,
                    ..Default::default()
                }
            ),
            EvaluationOutcome::Aborted {
                failure: EvaluationFailure::Budget(_),
                ..
            }
        ));
    }
    // Blank positions contribute separators when ignore_empty is false.
    assert_eq!(
        complete(run(
            &book,
            "TEXTJOIN(\".\",FALSE,A1:A3)",
            Default::default()
        )),
        CellValue::Text("first..".into())
    );
    // The first predicate admits implicit blanks, so they remain real matches.
    assert_eq!(
        complete(run(&book, "COUNTIF(A1:A3,\"\")", Default::default())),
        CellValue::Number(2.)
    );
}
#[test]
fn sparse_membership_invalidates_overlay_cache_after_insertions_and_deletions() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(target("C1").address, "CONCAT(A:A)")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(target("C2").address, "MAXIFS(B:B,A:A,\"last\")")
        .unwrap();
    let mut overlay = Overlay::with_workbook(book.model()).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C1"))
            .unwrap(),
        CellValue::Text("firstlast".into())
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C2"))
            .unwrap(),
        CellValue::Number(11.)
    );
    overlay.set_value(target("A2"), "last").unwrap();
    overlay.set_value(target("B2"), 17.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C1"))
            .unwrap(),
        CellValue::Text("firstlastlast".into())
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C2"))
            .unwrap(),
        CellValue::Number(17.)
    );
    overlay.set_value(target("A2"), CellValue::Blank).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C2"))
            .unwrap(),
        CellValue::Number(11.)
    );
}
#[test]
fn database_finalization_is_linear_and_keeps_sample_admission() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    for (address, value) in [
        ("A1", CellValue::Text("N".into())),
        ("A2", CellValue::Number(1.)),
        ("A3", CellValue::Number(2.)),
        ("A4", CellValue::Number(3.)),
        ("C1", CellValue::Text("N".into())),
    ] {
        sheet.set_value(address.parse().unwrap(), value).unwrap();
    }
    for (function, expected) in [
        ("DSUM", 6.),
        ("DAVERAGE", 2.),
        ("DVAR", 1.),
        ("DVARP", 2. / 3.),
    ] {
        let formula = format!("{function}(A1:A4,1,C1:C2)");
        let result = run(&book, &formula, EvaluationBudgets::default());
        assert!(result.usage().work_units < 1000, "{result:?}");
        assert_eq!(complete(result), CellValue::Number(expected));
    }
}
#[test]
fn wildcard_limits_count_visited_states_not_cross_product_guesses() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    sheet
        .set_value("A1".parse().unwrap(), "x".repeat(1000))
        .unwrap();
    let budgets = EvaluationBudgets {
        wildcard_states: 8,
        ..Default::default()
    };
    // The initial literal rejects immediately; the trailing star is never visited.
    assert_eq!(
        complete(run(&book, "COUNTIF(A1,\"a*\")", budgets)),
        CellValue::Number(0.)
    );
    // A leading star retries against the full input and exceeds the local state cap.
    assert!(matches!(
        run(&book, "IFERROR(COUNTIF(A1,\"*a\"),42)", budgets),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
    let outcome = run(&book, "COUNTIF(A1,\"*a\")", Default::default());
    assert!(outcome.usage().wildcard_states > 1000);
    let states = outcome.usage().wildcard_states;
    match run(
        &book,
        "COUNTIF(A1,\"*a\")",
        EvaluationBudgets {
            wildcard_states: states - 1,
            ..Default::default()
        },
    ) {
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(error),
            ..
        } => {
            assert_eq!(error.kind, eval::BudgetKind::WildcardStates);
            assert_eq!(error.consumed, states - 1);
            assert_eq!(error.requested, 1);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        complete(run(
            &book,
            "COUNTIF(A1,\"*a\")",
            EvaluationBudgets {
                wildcard_states: states,
                ..Default::default()
            }
        )),
        CellValue::Number(0.)
    );
    assert!(matches!(
        run(
            &book,
            "COUNTIF(A1,\"*a\")",
            EvaluationBudgets {
                wildcard_states: states - 1,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}
#[test]
fn scalar_rendering_obeys_actual_buffer_size_and_combina_charges_iterations() {
    let book = book();
    // These outputs fit in a few units; bounded numeric scratch is not a 512-cell array.
    for formula in ["1&2", "CONCAT(1,2)", "TEXTJOIN(\"\",TRUE,1,2)"] {
        assert_eq!(
            complete(run(
                &book,
                formula,
                EvaluationBudgets {
                    materialized_values: 8,
                    text_bytes: 64,
                    ..Default::default()
                }
            )),
            CellValue::Text("12".into())
        );
    }
    let cheap = run(&book, "COMBINA(2,1)", Default::default());
    let expensive = run(&book, "COMBINA(100,100)", Default::default());
    assert_eq!(complete(cheap.clone()), CellValue::Number(2.));
    assert!(expensive.usage().helper_operations > cheap.usage().helper_operations);
    assert!(matches!(
        run(
            &book,
            "IFERROR(COMBINA(100,100),42)",
            EvaluationBudgets {
                run_work: cheap.usage().work_units + 30,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
}
#[test]
fn direct_work_is_cumulative_until_begin_and_overlay_borrows_start_fresh() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(target("C1").address, "CONCAT(A:A)")
        .unwrap();
    let context = EvaluationContext {
        budgets: EvaluationBudgets {
            run_work: 500,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(book.model())
        .build(context.clone())
        .unwrap();
    let mut exhausted = false;
    for _ in 0..100 {
        direct.clear_cache();
        if direct.cell(target("C1")).is_err() {
            exhausted = true;
            break;
        }
    }
    assert!(exhausted, "clearing results must not replenish work");
    direct.begin(context.clone()).unwrap();
    assert_eq!(
        direct.cell(target("C1")).unwrap(),
        CellValue::Text("firstlast".into())
    );
    let mut overlay = Overlay::with_workbook(book.model()).build().unwrap();
    for _ in 0..3 {
        // Invalidating source forces recomputation, not merely a cached read.
        overlay.set_value(target("A1"), "first").unwrap();
        let mut evaluation = overlay.evaluation(&context).unwrap();
        assert_eq!(
            evaluation.cell(target("C1")).unwrap(),
            CellValue::Text("firstlast".into())
        );
        assert!(evaluation.finish().outcome.usage().work_units <= 500);
    }
}
