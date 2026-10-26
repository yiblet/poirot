//! Admission happens before cache cloning and keeps already completed entries.
use super::*;
use crate::eval::{EvaluationOutcome, compiler::FormulaCompiler};
use crate::model::{CellAddress, ExcelText, Workbook};
fn target(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse::<CellAddress>().unwrap(),
    }
}
#[test]
fn memo_limit_retains_partial_run_and_repeated_hits_cost_nothing() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet
        .set_value(target("A1").address, CellValue::Number(7.))
        .unwrap();
    sheet.set_formula(target("B1").address, "A1+1").unwrap();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let mut run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        EvaluationInputs::default(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_memo_limits(MemoRetentionLimits {
        cells: 1,
        bytes: 128,
    });
    assert_eq!(
        run.evaluate_cell(target("A1")).unwrap(),
        CellValue::Number(7.)
    );
    assert_eq!(
        run.evaluate_cell(target("A1")).unwrap(),
        CellValue::Number(7.)
    );
    let result = run.evaluate_cell(target("B1"));
    assert!(matches!(
        &result,
        Err(EvaluationFailure::MemoRetentionLimit { .. })
    ));
    let retained = run.finish_retaining_cells(result);
    assert_eq!(
        retained.memo_usage,
        MemoRetentionUsage {
            cells: 1,
            bytes: 128
        }
    );
    assert_eq!(retained.cells.len(), 1);
    assert_eq!(
        retained.cells.get(&(0, target("A1").address)),
        Some(&CellValue::Number(7.))
    );
    assert!(matches!(
        retained.report.outcome,
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::MemoRetentionLimit { .. },
            ..
        }
    ));
}
#[test]
fn utf16_payload_admission_is_independent_of_unlimited_calculation_budgets() {
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("S")
        .unwrap()
        .set_value(
            target("A1").address,
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800, 0, 0xffff])),
        )
        .unwrap();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let budgets = EvaluationBudgets {
        run_work: u64::MAX,
        cell_work: u64::MAX,
        materialized_values: usize::MAX,
        wildcard_states: u64::MAX,
        text_bytes: usize::MAX,
        ..EvaluationBudgets::default()
    };
    let mut run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        EvaluationInputs::default(),
        budgets,
    )
    .unwrap()
    .with_memo_limits(MemoRetentionLimits {
        cells: 1,
        bytes: 133,
    });
    let result = run.evaluate_cell(target("A1"));
    assert!(matches!(
        &result,
        Err(EvaluationFailure::MemoRetentionLimit {
            requested: MemoRetentionUsage {
                cells: 1,
                bytes: 134
            },
            ..
        })
    ));
    let retained = run.finish_retaining_cells(result);
    assert!(retained.cells.is_empty());
    assert_eq!(retained.memo_usage, MemoRetentionUsage::default());
}
