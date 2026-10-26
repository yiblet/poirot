use poirot::eval::{EvaluationBudgets, EvaluationInputs, EvaluationOutcome, run_formula};
use poirot::model::{CellAddress, CellError, CellValue, Workbook};
const AT: CellAddress = CellAddress { row: 2, col: 3 };
fn run(formula: &str, seed: u64, budgets: EvaluationBudgets) -> EvaluationOutcome<CellValue> {
    let mut book = Workbook::new();
    book.add_sheet("Sheet1").unwrap();
    run_formula(
        &book,
        0,
        AT,
        formula,
        EvaluationInputs {
            randomness_seed: seed,
            ..EvaluationInputs::default()
        },
        budgets,
    )
}
fn number(out: &EvaluationOutcome<CellValue>) -> f64 {
    match out {
        EvaluationOutcome::Complete {
            value: CellValue::Number(n),
            ..
        } => *n,
        other => panic!("{other:?}"),
    }
}
#[test]
fn actual_seeded_formula_draws_and_effects() {
    let a = run("RAND()", 17, EvaluationBudgets::default());
    assert_eq!(
        number(&a),
        number(&run("RAND()", 17, EvaluationBudgets::default()))
    );
    assert_ne!(
        number(&a),
        number(&run("RAND()", 18, EvaluationBudgets::default()))
    );
    assert!((0.0..1.0).contains(&number(&a)));
    let access = &a.usage().random_accesses[0];
    assert_eq!(
        (
            access.seed,
            access.row,
            access.column,
            access.draws,
            access.words
        ),
        (17, 2, 3, 1, 1)
    );
    let pair = run("RAND()+RAND()", 17, EvaluationBudgets::default());
    assert_eq!(pair.usage().random_accesses[0].draws, 2);
    assert_eq!(
        number(&pair),
        number(&run("RAND()+RAND()", 17, EvaluationBudgets::default()))
    );
    assert_eq!(
        number(&a),
        number(&run(
            "IF(FALSE,RAND(),RAND())",
            17,
            EvaluationBudgets::default()
        ))
    );
}
#[test]
fn intervals_precedence_and_preflight() {
    for (formula, expected) in [
        ("RANDBETWEEN(1.2,1.8)", 2.0),
        ("RANDBETWEEN(-3,-3)", -3.0),
        (
            "RANDBETWEEN(9007199254740992,9007199254740992)",
            9007199254740992.0,
        ),
    ] {
        assert_eq!(
            number(&run(formula, 5, EvaluationBudgets::default())),
            expected
        );
    }
    for (formula, error) in [
        ("RAND(1)", CellError::Value),
        ("RANDBETWEEN(3,2)", CellError::Num),
        // A numeric operand error propagates before any random draw (Calc 26.8.1.1).
        ("RANDBETWEEN(1/0,2)", CellError::Div0),
    ] {
        let out = run(formula, 5, EvaluationBudgets::default());
        assert!(
            matches!(out, EvaluationOutcome::Complete { value: CellValue::Error(e), .. } if e==error),
            "{formula}: {out:?}"
        );
        assert!(out.usage().random_accesses.is_empty());
    }
    let out = run(
        "IFERROR(RANDBETWEEN(-1e30,1e30),7)",
        5,
        EvaluationBudgets::default(),
    );
    assert!(matches!(out, EvaluationOutcome::Aborted { .. }));
    assert!(out.usage().random_accesses.is_empty());
    let budgets = EvaluationBudgets {
        run_work: 1,
        ..EvaluationBudgets::default()
    };
    let out = run("IFERROR(RAND(),7)", 5, budgets);
    assert!(matches!(out, EvaluationOutcome::Aborted { .. }));
    assert!(out.usage().random_accesses.is_empty());
}
#[test]
fn unrelated_order_and_memoized_references_preserve_streams() {
    let mut book = Workbook::new();
    book.add_sheet("Sheet1").unwrap();
    let sheet = book.sheet_mut(0).unwrap();
    sheet
        .set_formula_with_cached_value(
            CellAddress { row: 0, col: 0 },
            "RAND()",
            CellValue::Number(0.0),
        )
        .unwrap();
    sheet
        .set_formula_with_cached_value(
            CellAddress { row: 0, col: 1 },
            "RAND()",
            CellValue::Number(0.0),
        )
        .unwrap();
    let context = EvaluationInputs {
        randomness_seed: 73,
        ..EvaluationInputs::default()
    };
    let left = run_formula(
        &book,
        0,
        AT,
        "A1+B1+A1",
        context,
        EvaluationBudgets::default(),
    );
    let right = run_formula(
        &book,
        0,
        AT,
        "B1+A1+A1",
        context,
        EvaluationBudgets::default(),
    );
    assert_eq!(number(&left), number(&right));
    assert_eq!(left.usage().random_accesses.len(), 2);
    assert!(left.usage().random_accesses.iter().all(|a| a.draws == 1));
    assert!(left.usage().cache_hits >= 1);
}
#[test]
fn inclusive_endpoint_and_wide_interval_distribution() {
    let mut counts = [0usize; 3];
    for seed in 0..1024 {
        let out = run("RANDBETWEEN(-1,1)", seed, EvaluationBudgets::default());
        counts[(number(&out) + 1.0) as usize] += 1;
        let wide = number(&run(
            "RANDBETWEEN(-1000000000000,1000000000000)",
            seed,
            EvaluationBudgets::default(),
        ));
        assert_eq!(wide.fract(), 0.0);
        assert!((-1e12..=1e12).contains(&wide));
    }
    assert!(
        counts.iter().all(|&n| (250..440).contains(&n)),
        "{counts:?}"
    );
}

#[test]
fn distinct_stream_bookkeeping_has_linear_metered_work() {
    fn workbook_run(cells: u32) -> EvaluationOutcome<Vec<poirot::eval::CellEvaluation>> {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("Sheet1").unwrap();
        for row in 0..cells {
            sheet
                .set_formula(CellAddress { row, col: 0 }, "RAND()+RAND()")
                .unwrap();
        }
        poirot::eval::run_workbook(
            &book,
            EvaluationInputs {
                randomness_seed: 19,
                ..EvaluationInputs::default()
            },
            EvaluationBudgets::default(),
        )
    }
    let small = workbook_run(32);
    let large = workbook_run(64);
    for (outcome, count) in [(&small, 32usize), (&large, 64usize)] {
        assert!(matches!(outcome, EvaluationOutcome::Complete { .. }));
        assert_eq!(outcome.usage().random_accesses.len(), count);
        for (row, access) in outcome.usage().random_accesses.iter().enumerate() {
            assert_eq!(access.row, row as u32);
            assert_eq!(access.draws, 2);
            assert_eq!(access.words, 2);
        }
    }
    assert_eq!(
        large.usage().helper_operations,
        small.usage().helper_operations * 2
    );
    // Collection bounds are checked at their full size, while storage work
    // charges only newly inserted slots. Both counters therefore scale linearly.
    assert_eq!(
        large.usage().materialized_values,
        small.usage().materialized_values * 2
    );
    assert_eq!(large.usage().work_units, small.usage().work_units * 2);
}
