//! Defaults disable runtime policy caps while explicit limits remain enforceable.
use poirot::eval::{
    self, EvaluationBudgets, EvaluationFailure, EvaluationInputs, EvaluationOutcome,
    EvaluationTarget, run::MemoRetentionLimits,
};
use poirot::{CellValue, EvaluationContext, Workbook};
fn target(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
#[test]
fn runtime_and_retention_defaults_are_unlimited_but_depth_remains_structural() {
    let budgets = EvaluationBudgets::default();
    assert_eq!(budgets.run_work, u64::MAX);
    assert_eq!(budgets.cell_work, u64::MAX);
    assert_eq!(budgets.materialized_values, usize::MAX);
    assert_eq!(budgets.text_bytes, usize::MAX);
    assert_eq!(budgets.wildcard_states, u64::MAX);
    assert_eq!(budgets.depth, 128);
    assert_eq!(
        MemoRetentionLimits::default(),
        MemoRetentionLimits {
            cells: usize::MAX,
            bytes: usize::MAX
        }
    );
    assert_eq!(EvaluationContext::default().budgets, budgets);
}
#[test]
fn default_direct_overlay_and_low_level_runs_accept_text_beyond_the_old_cap() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    const LENGTH: usize = 1_048_577;
    sheet
        .set_value(target("A1").address, "a".repeat(LENGTH))
        .unwrap();
    sheet.set_formula(target("B1").address, "LEN(A1)").unwrap();
    let expected = CellValue::Number(LENGTH as f64);
    let mut direct = poirot::Evaluation::with_workbook(book.model())
        .build(Default::default())
        .unwrap();
    assert_eq!(direct.cell(target("B1")).unwrap(), expected);
    assert_eq!(direct.cell(target("B1")).unwrap(), expected);
    let mut overlay = poirot::Overlay::with_workbook(book.model())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B1"))
            .unwrap(),
        expected
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B1"))
            .unwrap(),
        expected
    );
    assert!(
        matches!(eval::run_formula(book.model(), 0, target("B1").address, "LEN(A1)", EvaluationInputs::default(), EvaluationBudgets::default()), EvaluationOutcome::Complete { value, .. } if value == expected)
    );
    // Cached values do not waive newly configured capacities.
    let limited = EvaluationContext {
        budgets: EvaluationBudgets {
            text_bytes: LENGTH - 1,
            ..Default::default()
        },
        ..context.clone()
    };
    assert!(matches!(
        overlay.evaluation(&limited).unwrap().cell(target("A1")),
        Err(EvaluationFailure::Budget(_))
    ));
    direct.begin(limited).unwrap();
    assert!(matches!(
        direct.cell(target("B1")),
        Err(EvaluationFailure::Budget(_))
    ));
    let limited = EvaluationContext {
        retention: MemoRetentionLimits { cells: 0, bytes: 0 },
        ..context
    };
    direct.begin(limited.clone()).unwrap();
    assert!(matches!(
        direct.cell(target("B1")),
        Err(EvaluationFailure::MemoRetentionLimit { .. })
    ));
    assert!(matches!(
        overlay.evaluation(&limited).unwrap().cell(target("A1")),
        Err(EvaluationFailure::MemoRetentionLimit { .. })
    ));
}
