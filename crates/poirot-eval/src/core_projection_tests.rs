use super::*;
fn eval(formula: &str, literal: CellValue, budgets: EvaluationBudgets) -> EvalResult<CellValue> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Data").unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), literal)
        .unwrap();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.budget = BudgetMeter::new(budgets).unwrap();
    let at = "B1".parse().unwrap();
    let expression = crate::bound::bind(&poirot_formula::parse_formula(formula).unwrap());
    let value = session.expr(0, at, &expression)?;
    session.scalar(at, value)
}
#[test]
fn malformed_scalar_projection_is_operational_and_uncatchable() {
    let lone = || CellValue::TextUtf16(poirot_model::ExcelText::from_units(vec![0xd83d]));
    for formula in ["IFERROR(A1+1,7)", "ISERROR(A1+1)", "IFERROR(VALUE(A1),7)"] {
        assert!(
            matches!(
                eval(formula, lone(), EvaluationBudgets::default()),
                Err(EvaluationFailure::UnsupportedCapability { .. })
            ),
            "{formula}"
        );
    }
    // These source consumers use units/types, so no scalar projection occurs.
    for (formula, expected) in [
        ("A1=A1", Boolean(true)),
        ("A1=1", Boolean(false)),
        ("IF(A1,1,2)", Error(VALUE_ERROR)),
        ("AND(A1:A1)", Error(VALUE_ERROR)),
        ("IF(FALSE,A1+1,7)", Number(7.)),
    ] {
        assert_eq!(
            eval(formula, lone(), EvaluationBudgets::default()).unwrap(),
            expected,
            "{formula}"
        );
    }
}
#[test]
fn valid_manual_utf16_has_identical_scalar_semantics() {
    for text in ["12", "TRUE", "abc", "é", "😀"] {
        for formula in ["IFERROR(A1+1,7)", "IFERROR(NOT(A1),7)", "A1=A1"] {
            let plain = eval(formula, Text(text.into()), EvaluationBudgets::default());
            let units = eval(
                formula,
                CellValue::TextUtf16(poirot_model::ExcelText::from(text)),
                EvaluationBudgets::default(),
            );
            assert_eq!(plain, units, "{formula}: {text}");
        }
    }
    // Semantically invalid numeric text still produces VALUE and is catchable.
    assert_eq!(
        eval(
            "IFERROR(A1+1,7)",
            Text("nonnumeric".into()),
            EvaluationBudgets::default()
        )
        .unwrap(),
        Number(7.)
    );
}
#[test]
fn exact_projection_admission_precedes_allocation() {
    let value = CellValue::TextUtf16(poirot_model::ExcelText::from("1234"));
    let budgets = EvaluationBudgets {
        text_bytes: 3,
        ..EvaluationBudgets::default()
    };
    assert!(matches!(
        eval("IFERROR(A1+1,7)", value, budgets),
        Err(EvaluationFailure::Budget(_))
    ));
}
#[test]
fn formatter_failure_causes_remain_distinct() {
    use poirot_format::{FormatError, FormatErrorKind};
    let format_error = |kind| FormatError {
        kind,
        position: 17,
        message: "retained source resource ceiling".into(),
    };
    assert!(matches!(
        format_failure(format_error(FormatErrorKind::InvalidFormat)),
        FunctionFailure::Spreadsheet(VALUE_ERROR)
    ));
    assert!(matches!(
        format_failure(format_error(FormatErrorKind::UnsupportedFidelity)),
        FunctionFailure::Operational(EvaluationFailure::UnsupportedCapability {
            capability: CapabilityId::Formatting,
            ..
        })
    ));
    let FunctionFailure::Operational(mut failure) =
        format_failure(format_error(FormatErrorKind::ResourceLimit))
    else {
        panic!("resource errors must be operational");
    };
    assert!(failure.budget().is_none());
    let target = EvaluationTarget {
        sheet: 0,
        address: "B1".parse().unwrap(),
    };
    failure.set_target(target);
    assert!(
        matches!(&failure, EvaluationFailure::ResourceLimit { source: Some(at), detail, .. } if *at == target && detail.contains("at byte 17"))
    );
    assert!(
        failure
            .to_string()
            .contains("retained source resource ceiling")
    );
}

#[test]
fn formatted_number_work_admission_is_operational_and_utf16_equivalent() {
    let limits = EvaluationBudgets {
        run_work: 100,
        cell_work: 100,
        ..EvaluationBudgets::default()
    };
    for formula in [
        "IFERROR(A1+1,7)",
        "IFERROR(VALUE(A1),7)",
        "IFERROR(ABS(A1),7)",
    ] {
        assert!(
            matches!(
                eval(formula, Text("1,234.50%".into()), limits),
                Err(EvaluationFailure::Budget(_))
            ),
            "{formula}"
        );
    }
    for formula in ["A1+1", "VALUE(A1)", "ABS(A1)"] {
        let expected = eval(
            formula,
            Text("1,234.50%".into()),
            EvaluationBudgets::default(),
        )
        .unwrap();
        assert_eq!(
            eval(
                formula,
                CellValue::TextUtf16(poirot_model::ExcelText::from("1,234.50%")),
                EvaluationBudgets::default()
            )
            .unwrap(),
            expected
        );
    }
}
