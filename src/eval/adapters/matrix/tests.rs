use crate::eval::config::EvaluationInputs;
use crate::eval::limits::{BudgetMeter, EvalResult, EvaluationBudgets, EvaluationFailure};
use crate::eval::session::{ResultProjection, Session};
use crate::eval::vm::ExpressionRunner;
use crate::model::CellValue::{Number, Text};
use crate::model::{CellValue, Workbook};

fn evaluate(
    formula: &str,
    literal: CellValue,
    budgets: EvaluationBudgets,
) -> EvalResult<CellValue> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Data").unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), literal)
        .unwrap();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.state.runtime.budget = BudgetMeter::new(budgets).unwrap();
    let at = "B1".parse().unwrap();
    let expression = crate::eval::compile::bound::bind(
        &poirot_parsing::formula::parse_formula(formula).unwrap(),
    );
    let value = ExpressionRunner::new(&mut session).expr(0, at, &expression)?;
    ResultProjection::new(&mut session).select_legacy_scalar(at, value)
}

#[test]
fn consumed_helper_text_preserves_operational_projection_failure() {
    for formula in ["IFERROR(PV(A1,1,1),7)", "IFERROR(NORMDIST(A1,0,1,TRUE),7)"] {
        let literal = CellValue::TextUtf16(crate::model::ExcelText::from_units(vec![0xd83d]));
        assert!(
            matches!(
                evaluate(formula, literal, EvaluationBudgets::default()),
                Err(EvaluationFailure::UnsupportedCapability { .. })
            ),
            "{formula}"
        );
    }
}

#[test]
fn valid_helper_text_projection_matches_utf8_and_obeys_budget() {
    for formula in ["PV(A1,1,1)", "NORMDIST(A1,0,1,TRUE)", "MATCH(A1,A1:A1,0)"] {
        let plain = evaluate(formula, Text("12".into()), EvaluationBudgets::default());
        let projected = evaluate(
            formula,
            CellValue::TextUtf16(crate::model::ExcelText::from("12")),
            EvaluationBudgets::default(),
        );
        assert_eq!(plain, projected, "{formula}");
    }
    let budgets = EvaluationBudgets {
        text_bytes: 1,
        ..EvaluationBudgets::default()
    };
    assert!(matches!(
        evaluate(
            "IFERROR(PV(A1,1,1),7)",
            CellValue::TextUtf16(crate::model::ExcelText::from("12")),
            budgets,
        ),
        Err(EvaluationFailure::Budget(_))
    ));
    assert_eq!(
        evaluate(
            "CHOOSE(1,7,A1)",
            CellValue::TextUtf16(crate::model::ExcelText::from_units(vec![0xd83d])),
            EvaluationBudgets::default(),
        )
        .unwrap(),
        Number(7.)
    );
}
