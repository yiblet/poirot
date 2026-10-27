//! Borrowed source reads preserve range values across saved ownership and sparse edits.
//! SUM ignores blank references, while SEQUENCE(2) fills two vertical coordinates.
//! These contracts are documented in Microsoft's SUM and SEQUENCE function references.
use crate::eval::compile::compiler::{FormulaCompiler, PreparationPolicy};
use crate::eval::config::EvaluationContext;
use crate::eval::config::{ArrayResultPolicy, RunConfiguration};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{EvaluationFailure, EvaluationOutcome};
use crate::eval::overlay::{Evaluation, Overlay};
use crate::eval::ports::{CellCompletion, CellObserver};
use crate::eval::run::{EvaluationRun, WorkbookView};
use crate::model::{CellValue, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

fn source_book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula(at("A1").address, "SEQUENCE(2)").unwrap();
    sheet.set_value(at("A2").address, CellValue::Blank).unwrap();
    sheet
        .set_spill_cached_value(
            at("A2").address,
            at("A1").address,
            "SEQUENCE(2)",
            99.0.into(),
        )
        .unwrap();
    sheet.set_formula(at("A3").address, "C1+1").unwrap();
    sheet.set_value(at("A4").address, 5.).unwrap();
    sheet.set_value(at("C1").address, 3.).unwrap();
    sheet.set_formula(at("D1").address, "SUM(A2:A5)").unwrap();
    book
}

#[test]
fn ranged_formulas_saved_children_and_blanks_follow_effective_source_edits() {
    let book = source_book();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    for (step, expected) in [(0, 11.), (1, 18.), (2, 14.), (3, 7.), (4, 11.)] {
        match step {
            1 => {
                direct.set_value(at("A2"), 9.).unwrap();
                overlay.set_value(at("A2"), 9.).unwrap();
            }
            2 => {
                direct.remove_cell(at("A3")).unwrap();
                overlay.remove_cell(at("A3")).unwrap();
            }
            3 => {
                direct.remove_cell(at("A2")).unwrap();
                overlay.remove_cell(at("A2")).unwrap();
            }
            4 => {
                direct.discard_edits();
                overlay.discard_edits();
            }
            _ => {}
        }
        // The stale saved child (99) never substitutes for the current SEQUENCE
        // result (2). A literal override blocks the producer but remains readable.
        assert_eq!(direct.cell(at("D1")).unwrap(), expected.into());
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("D1"))
                .unwrap(),
            expected.into()
        );
    }
}

/// Count completions so the observed range follows the ordinary cell protocol.
#[derive(Default)]
struct Completions(Vec<EvaluationTarget>);

impl CellObserver for Completions {
    fn completed(&mut self, event: CellCompletion<'_>) -> Result<(), EvaluationFailure> {
        self.0.push(event.target);
        Ok(())
    }
}

#[test]
fn observed_ranges_keep_the_same_source_values_and_report_member_completion() {
    let book = source_book();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
    let config =
        RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::DynamicSpill);
    let mut completions = Completions::default();
    let mut run = EvaluationRun::new_with_configuration(
        WorkbookView::new(&book, &plan, &policy).unwrap(),
        &config,
        Default::default(),
    )
    .unwrap()
    .with_cell_observer(&mut completions);
    assert_eq!(run.evaluate_cell(at("D1")).unwrap(), 11.0.into());
    assert_eq!(run.evaluate_cell(at("A2")).unwrap(), 2.0.into());
    assert!(matches!(
        run.finish(Ok(())).outcome,
        EvaluationOutcome::Complete { .. }
    ));
    for member in ["A2", "A3", "A4", "D1"] {
        assert!(completions.0.contains(&at(member)), "{member} completion");
    }
}
