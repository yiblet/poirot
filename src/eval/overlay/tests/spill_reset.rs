//! Reset restores baseline spill producers and preserves unchanged classification proofs.
//! Expected arrays follow SEQUENCE's documented row-major sequence; these examples
//! and reset controls are original, not copied from another implementation.
use crate::eval::config::EvaluationContext;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::overlay::compilation_usage::CompilationUsage;
use crate::eval::overlay::evaluation::Evaluation;
use crate::eval::overlay::incremental::Overlay;
use crate::model::{CellStyle, CellValue, Workbook};

fn at(sheet: usize, address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet,
        address: address.parse().unwrap(),
    }
}

/// Public direct and incremental workspaces exercised by identical reset scenarios.
enum Workspace<'a> {
    Direct(Box<Evaluation<'a>>),
    Incremental(Box<Overlay<'a>>),
}

impl<'a> Workspace<'a> {
    fn new(book: &'a Workbook, incremental: bool) -> Self {
        if incremental {
            Self::Incremental(Box::new(Overlay::with_workbook(book).build().unwrap()))
        } else {
            Self::Direct(Box::new(
                Evaluation::with_workbook(book)
                    .build(Default::default())
                    .unwrap(),
            ))
        }
    }

    fn cell(&mut self, target: EvaluationTarget) -> CellValue {
        match self {
            Self::Direct(eval) => eval.cell(target).unwrap(),
            Self::Incremental(overlay) => overlay
                .evaluation(&EvaluationContext::default())
                .unwrap()
                .cell(target)
                .unwrap(),
        }
    }

    fn blank_with_small_work_budget(&mut self, target: EvaluationTarget) {
        let mut context = EvaluationContext::default();
        // This admits normal blank resolution but not rescanning the 64 formulas.
        context.limits = context.limits.max_work(8);
        let value = match self {
            Self::Direct(eval) => {
                eval.begin(context).unwrap();
                eval.cell(target).unwrap()
            }
            Self::Incremental(overlay) => {
                overlay.evaluation(&context).unwrap().cell(target).unwrap()
            }
        };
        assert_eq!(value, CellValue::Blank);
    }

    fn formula(&mut self, target: EvaluationTarget, formula: &str) {
        match self {
            Self::Direct(eval) => eval.set_formula(target, formula).unwrap(),
            Self::Incremental(overlay) => overlay.set_formula(target, formula).unwrap(),
        }
    }

    fn value(&mut self, target: EvaluationTarget, value: f64) {
        match self {
            Self::Direct(eval) => eval.set_value(target, value).unwrap(),
            Self::Incremental(overlay) => overlay.set_value(target, value).unwrap(),
        }
    }

    fn remove(&mut self, target: EvaluationTarget) {
        match self {
            Self::Direct(eval) => eval.remove_cell(target).unwrap(),
            Self::Incremental(overlay) => overlay.remove_cell(target).unwrap(),
        }
    }

    fn metadata(&mut self, target: EvaluationTarget) {
        let style = CellStyle {
            number_format: Some("0.00".into()),
            ..Default::default()
        };
        match self {
            Self::Direct(eval) => {
                eval.set_style(target, style).unwrap();
                eval.set_cached_value(target, 999.).unwrap();
            }
            Self::Incremental(overlay) => {
                overlay.set_style(target, style).unwrap();
                overlay.set_cached_value(target, 999.).unwrap();
            }
        }
    }

    fn reset(&mut self) {
        match self {
            Self::Direct(eval) => eval.reset(),
            Self::Incremental(overlay) => overlay.reset(),
        }
    }

    fn usage(&mut self) -> CompilationUsage {
        match self {
            Self::Direct(eval) => eval.compiled_usage(),
            Self::Incremental(overlay) => overlay.compiled_usage(),
        }
    }
}

#[test]
fn reset_removes_a_temporary_producer_before_child_first_reads() {
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula(at(0, "D1").address, "A2*2")
        .unwrap();
    for incremental in [false, true] {
        let mut eval = Workspace::new(&book, incremental);
        eval.formula(at(0, "A1"), "SEQUENCE(2)");
        assert_eq!(eval.cell(at(0, "D1")), 4.0.into());
        eval.reset();
        assert_eq!(eval.cell(at(0, "A2")), CellValue::Blank);
        assert_eq!(eval.cell(at(0, "D1")), 0.0.into());
    }
}

#[test]
fn reset_restores_a_baseline_producer_after_scalar_override_or_deletion() {
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula(at(0, "A1").address, "SEQUENCE(2)")
        .unwrap();
    for incremental in [false, true] {
        for deletion in [false, true] {
            let mut eval = Workspace::new(&book, incremental);
            if deletion {
                eval.remove(at(0, "A1"));
            } else {
                eval.value(at(0, "A1"), 7.);
            }
            assert_eq!(eval.cell(at(0, "A2")), CellValue::Blank);
            eval.reset();
            assert_eq!(eval.cell(at(0, "A2")), 2.0.into());
        }
    }
}

#[test]
fn reset_preserves_other_sheet_and_literal_style_cached_value_classifications() {
    let mut book = Workbook::new();
    book.add_sheet("Edited").unwrap();
    let stable = book.add_sheet("Stable").unwrap();
    for row in 1..=64 {
        stable
            .set_formula(at(1, &format!("A{row}")).address, format!("{row}+1"))
            .unwrap();
    }
    stable.set_value(at(1, "D1").address, 3.).unwrap();
    let mut limited = EvaluationContext::default();
    limited.limits = limited.limits.max_work(8);
    let mut cold_direct = Evaluation::with_workbook(&book)
        .build(limited.clone())
        .unwrap();
    assert!(
        matches!(cold_direct.cell(at(1, "B65")), Err(crate::eval::limits::EvaluationError::Budget(failure)) if failure.kind == crate::eval::limits::BudgetKind::RunWork)
    );
    let mut cold_overlay = Overlay::with_workbook(&book).build().unwrap();
    assert!(
        matches!(cold_overlay.evaluation(&limited).unwrap().cell(at(1, "B65")), Err(crate::eval::limits::EvaluationError::Budget(failure)) if failure.kind == crate::eval::limits::BudgetKind::RunWork)
    );
    for incremental in [false, true] {
        let mut eval = Workspace::new(&book, incremental);
        assert_eq!(eval.cell(at(1, "B65")), CellValue::Blank);
        eval.formula(at(0, "A1"), "SEQUENCE(2)");
        assert_eq!(eval.cell(at(0, "A2")), 2.0.into());
        eval.reset();
        let before = eval.usage();
        eval.blank_with_small_work_budget(at(1, "B65"));
        let after = eval.usage();
        assert_eq!(
            after.reused_formulas, before.reused_formulas,
            "unmodified sheet must keep its classification coverage"
        );
        assert_eq!(after.parse_attempts, before.parse_attempts);
        if let Workspace::Direct(direct) = &mut eval {
            direct.begin(EvaluationContext::default()).unwrap();
        }
        eval.value(at(1, "D1"), 4.);
        eval.metadata(at(1, "A1"));
        assert_eq!(eval.cell(at(1, "B65")), CellValue::Blank);
        eval.reset();
        let before = eval.usage();
        eval.blank_with_small_work_budget(at(1, "B65"));
        let after = eval.usage();
        assert_eq!(
            after.reused_formulas, before.reused_formulas,
            "literal and metadata resets must keep formula classification coverage"
        );
        assert_eq!(after.parse_attempts, before.parse_attempts);
    }
}

#[test]
fn flush_keeps_committed_producer_classification_and_future_resets() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    let mut overlay = Overlay::with_workbook(&mut book).build().unwrap();
    let context = EvaluationContext::default();
    overlay.set_formula(at(0, "A1"), "SEQUENCE(2)").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at(0, "A2"))
            .unwrap(),
        2.0.into()
    );
    assert_eq!(overlay.flush().unwrap(), 1);
    overlay.set_value(at(0, "A1"), 9.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at(0, "A2"))
            .unwrap(),
        CellValue::Blank
    );
    overlay.reset();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at(0, "A2"))
            .unwrap(),
        2.0.into()
    );
}

#[test]
fn scalar_array_and_blocked_results_replace_complete_footprints() {
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula(at(0, "A1").address, "7")
        .unwrap();
    for incremental in [false, true] {
        let mut eval = Workspace::new(&book, incremental);
        assert_eq!(eval.cell(at(0, "A1")), 7.0.into());
        assert_eq!(eval.cell(at(0, "B2")), CellValue::Blank);
        eval.formula(at(0, "A1"), "{1,2;3,4}");
        assert_eq!(eval.cell(at(0, "B2")), 4.0.into());
        assert_eq!(eval.cell(at(0, "A1")), 1.0.into());
        // A source value blocks the entire new array, including its anchor.
        eval.value(at(0, "A2"), 99.);
        assert_eq!(
            eval.cell(at(0, "A1")),
            CellValue::Error(crate::model::CellError::Spill)
        );
        assert_eq!(eval.cell(at(0, "B2")), CellValue::Blank);
        eval.remove(at(0, "A2"));
        assert_eq!(eval.cell(at(0, "B2")), 4.0.into());
        // Formula blank projection is zero, while the old footprint disappears.
        eval.formula(at(0, "A1"), "Z1");
        assert_eq!(eval.cell(at(0, "A1")), 0.0.into());
        assert_eq!(eval.cell(at(0, "A2")), CellValue::Blank);
        assert_eq!(eval.cell(at(0, "B2")), CellValue::Blank);
        eval.reset();
        assert_eq!(eval.cell(at(0, "A1")), 7.0.into());
        assert_eq!(eval.cell(at(0, "B2")), CellValue::Blank);
    }
}
