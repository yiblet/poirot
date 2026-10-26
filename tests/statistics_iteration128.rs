//! Prospective actual-handler resource regressions; central139 owns execution.
use poirot::eval::{
    self as eval, EvaluationBudgets, EvaluationFailure, EvaluationInputs, EvaluationOutcome,
};
use poirot::model::{CellAddress, CellError, CellValue, Workbook};

const AT: CellAddress = CellAddress { row: 0, col: 0 };

fn run(book: &Workbook, formula: &str, work: u64) -> EvaluationOutcome<CellValue> {
    let expression = poirot_parsing::formula::parse_formula(formula).unwrap();
    let plan = eval::bound::FormulaPlan::bind(&expression).unwrap();
    eval::run_plan(
        book,
        0,
        AT,
        &plan,
        EvaluationInputs::default(),
        EvaluationBudgets {
            run_work: work,
            ..EvaluationBudgets::default()
        },
    )
}

#[test]
fn poisson_iterations_abort_through_actual_handler_and_iferror() {
    let mut book = Workbook::new();
    book.add_sheet("Sheet1").unwrap();
    // Establish enough budget for acquisition and the bounded scalar setup.
    // The large near-mean P series takes many more iterations than this control.
    let control = run(&book, "IFERROR(POISSON(0,1,TRUE),42)", 1_000_000);
    assert!(matches!(
        &control,
        EvaluationOutcome::Complete {
            value: CellValue::Number(_),
            ..
        }
    ));
    let budget = control.usage().work_units + 128;
    for name in ["POISSON", "POISSON.DIST", "_xlfn.POISSON.DIST"] {
        let formula = format!("IFERROR({name}(1000000,1000000,TRUE),42)");
        let outcome = run(&book, &formula, budget);
        assert!(
            matches!(
                outcome,
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::Budget(_),
                    ..
                }
            ),
            "{formula}"
        );
    }
}

#[test]
fn normal_gamma_iterations_are_charged_after_the_fixed_overhead() {
    let mut book = Workbook::new();
    book.add_sheet("Sheet1").unwrap();
    for name in ["NORMSDIST", "NORM.S.DIST", "_xlfn.NORM.S.DIST"] {
        let center = format!(
            "IFERROR({name}({}),42)",
            if name == "NORMSDIST" { "0" } else { "0,TRUE" }
        );
        let control = run(&book, &center, 1_000_000);
        assert!(matches!(
            &control,
            EvaluationOutcome::Complete {
                value: CellValue::Number(0.5),
                ..
            }
        ));
        let formula = format!(
            "IFERROR({name}({}),42)",
            if name == "NORMSDIST" { "1" } else { "1,TRUE" }
        );
        assert!(
            matches!(
                run(&book, &formula, control.usage().work_units + 2),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::Budget(_),
                    ..
                }
            ),
            "{formula}"
        );
    }
}

#[test]
fn inverse_invalid_probability_stays_a_spreadsheet_error() {
    let mut book = Workbook::new();
    book.add_sheet("Sheet1").unwrap();
    for name in ["NORMINV", "NORM.INV", "_xlfn.NORM.INV"] {
        let formula = format!("{name}(-0,0,1)");
        assert!(
            matches!(
                run(&book, &formula, 128),
                EvaluationOutcome::Complete {
                    value: CellValue::Error(CellError::Num),
                    ..
                }
            ),
            "{formula}"
        );
        let formula = format!("IFERROR({name}(-0,0,1),42)");
        assert!(
            matches!(
                run(&book, &formula, 128),
                EvaluationOutcome::Complete {
                    value: CellValue::Number(42.0),
                    ..
                }
            ),
            "{formula}"
        );
    }
}

#[test]
fn zero_mean_poisson_is_a_degenerate_finite_distribution() {
    // Microsoft's mass/cumulative equations place all probability at zero.
    // https://support.microsoft.com/en-us/excel/functions/poisson-dist-function
    let mut book = Workbook::new();
    book.add_sheet("Sheet1").unwrap();
    for name in ["POISSON", "POISSON.DIST", "_xlfn.POISSON.DIST"] {
        for (arguments, expected) in [
            ("0,0,FALSE", 1.),
            ("0.1,0,FALSE", 1.),
            ("1,0,FALSE", 0.),
            ("1.9,0,FALSE", 0.),
            ("1,0,TRUE", 1.),
        ] {
            let formula = format!("{name}({arguments})");
            assert!(
                matches!(run(&book,&formula,100_000),EvaluationOutcome::Complete {value:CellValue::Number(n),..} if n==expected),
                "{formula}"
            );
        }
    }
}
