//! Public lifecycle regressions across direct and incremental evaluation.
//! Expectations use hand arithmetic and the root API's documented division-by-zero contract.
use poirot::{CellError, CellValue, EvaluationContext, Workbook, eval};

/// Build a dependency diamond with lazy error handling and deliberately stale saved results.
fn workbook() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Model").unwrap();
    sheet.set_value("A1".parse().unwrap(), 3.).unwrap();
    for (address, formula) in [
        ("B1", "A1*2"),
        ("C1", "B1+1"),
        ("D1", "IF(A1>0,B1+C1,1/0)"),
        ("E1", "IFERROR(D1,99)"),
    ] {
        sheet
            .set_formula_with_cached_value(
                address.parse().unwrap(),
                formula,
                CellValue::Number(-1.),
            )
            .unwrap();
    }
    book
}

#[test]
fn dependent_reads_match_across_modes_warm_reuse_and_input_edits() {
    let book = workbook();
    let context = EvaluationContext::default();
    let mut direct = book.evaluation().context(context.clone()).build().unwrap();
    let mut overlay = book.overlay().build().unwrap();
    let input = direct.target("Model!A1").unwrap();
    let output = direct.target("Model!D1").unwrap();
    let recovered = direct.target("Model!E1").unwrap();
    // 2a + (2a + 1) = 4a + 1. A negative input selects the division-by-zero branch.
    for (input_value, expected) in [
        (3., CellValue::Number(13.)),
        (4., CellValue::Number(17.)),
        (-1., CellValue::Error(CellError::Div0)),
        (5., CellValue::Number(21.)),
    ] {
        direct.set_value(input, input_value).unwrap();
        overlay.set_value(input, input_value).unwrap();
        assert_eq!(direct.cell(output).unwrap(), expected);
        let recovered_expected = match &expected {
            CellValue::Error(_) => CellValue::Number(99.),
            value => value.clone(),
        };
        assert_eq!(direct.cell(recovered).unwrap(), recovered_expected);
        {
            let mut cycle = overlay.evaluation(&context).unwrap();
            assert_eq!(cycle.cell(output).unwrap(), expected);
            assert_eq!(cycle.cell(recovered).unwrap(), recovered_expected);
        }
        // A new incremental cycle must reuse the completed output without re-execution.
        let mut warm = overlay.evaluation_with_profiling(&context, true).unwrap();
        assert_eq!(warm.cell(output).unwrap(), expected);
        assert_eq!(warm.cell(recovered).unwrap(), recovered_expected);
        let execution = warm.finish().execution.unwrap();
        assert_eq!(execution.formula_executions, 0);
        assert!(execution.external_cache_hits >= 2);
    }
    assert_eq!(
        book.stored_cell("Model", output.address).unwrap().value,
        CellValue::Number(-1.)
    );
}

#[test]
fn aborted_output_batch_publishes_no_dependencies_and_next_cycle_recovers() {
    let mut book = workbook();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("F1".parse().unwrap(), "REPT(\"x\",64)")
        .unwrap();
    let mut overlay = book.overlay().build().unwrap();
    let output = overlay.target("Model!D1").unwrap();
    let oversized = overlay.target("Model!F1").unwrap();
    let limited = EvaluationContext {
        budgets: eval::EvaluationBudgets {
            text_bytes: 32,
            ..Default::default()
        },
        ..Default::default()
    };
    {
        let mut cycle = overlay.evaluation(&limited).unwrap();
        let failure = cycle.outputs(&[output, oversized]).unwrap_err();
        // The completed first output cannot escape the failed batch; failures stay sticky.
        assert_eq!(cycle.cell(output).unwrap_err(), failure);
    }
    assert_eq!(overlay.cached_cells(), 0);
    assert!(overlay.cached_value(output).is_none());
    assert!(overlay.changes().is_empty());
    let context = EvaluationContext::default();
    let mut recovered = overlay.evaluation(&context).unwrap();
    assert_eq!(recovered.cell(output).unwrap(), CellValue::Number(13.));
    assert_eq!(
        recovered.cell(oversized).unwrap(),
        CellValue::Text("x".repeat(64))
    );
}

#[test]
fn direct_dependency_depth_failure_restores_the_next_cycle() {
    let book = workbook();
    let limited = EvaluationContext {
        budgets: eval::EvaluationBudgets {
            depth: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut direct = book.evaluation().context(limited).build().unwrap();
    let output = direct.target("Model!D1").unwrap();
    let failure = direct.cell(output).unwrap_err();
    assert_eq!(direct.cell(output).unwrap_err(), failure);
    assert!(direct.changes().is_empty());
    direct.start_cycle(EvaluationContext::default()).unwrap();
    assert_eq!(direct.cell(output).unwrap(), CellValue::Number(13.));
    assert_eq!(direct.cell(output).unwrap(), CellValue::Number(13.));
}
