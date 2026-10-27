use super::super::value::VALUE_ERROR;
use super::scalar::format_failure;
use crate::eval::config::{DateOrder, EvaluationInputs, Localization};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{
    BudgetMeter, CapabilityId, EvalResult, EvaluationBudgets, EvaluationError,
};
use crate::eval::session::{Coercer, ResultProjection, Session};
use crate::eval::values::function_error::FunctionError;
use crate::eval::vm::ExpressionRunner;
use crate::model::CellValue::{Boolean, Error, Number, Text};
use crate::model::{CellError, CellValue, Workbook};
use std::time::{Duration, UNIX_EPOCH};

fn eval(formula: &str, literal: CellValue, budgets: EvaluationBudgets) -> EvalResult<CellValue> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Data").unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value("A1", literal)
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
fn malformed_scalar_projection_is_operational_and_uncatchable() {
    let lone = || CellValue::TextUtf16(crate::model::ExcelText::from_units(vec![0xd83d]));
    for formula in ["IFERROR(A1+1,7)", "ISERROR(A1+1)", "IFERROR(VALUE(A1),7)"] {
        assert!(
            matches!(
                eval(formula, lone(), EvaluationBudgets::default()),
                Err(EvaluationError::UnsupportedCapability { .. })
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
                CellValue::TextUtf16(crate::model::ExcelText::from(text)),
                EvaluationBudgets::default(),
            );
            assert_eq!(plain.unwrap(), units.unwrap(), "{formula}: {text}");
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
    let value = CellValue::TextUtf16(crate::model::ExcelText::from("1234"));
    let budgets = EvaluationBudgets {
        text_bytes: 3,
        ..EvaluationBudgets::default()
    };
    assert!(matches!(
        eval("IFERROR(A1+1,7)", value, budgets),
        Err(EvaluationError::Budget(_))
    ));
}

#[test]
fn formatter_failure_causes_remain_distinct() {
    use poirot_functions::format::{FormatError, FormatErrorKind, FormatReasonError};
    let format_error = |kind| FormatError {
        kind,
        position: 17,
        reason: match kind {
            FormatErrorKind::InvalidFormat => FormatReasonError::FormatHasNoSections,
            FormatErrorKind::UnsupportedFidelity => {
                FormatReasonError::DateSerialOutsideSupportedNonnegativeCalendarRange
            }
            FormatErrorKind::ResourceLimit => FormatReasonError::FormatExceedsResourceLimit,
        },
    };
    assert!(matches!(
        format_failure(format_error(FormatErrorKind::InvalidFormat)),
        FunctionError::Spreadsheet(VALUE_ERROR)
    ));
    assert!(matches!(
        format_failure(format_error(FormatErrorKind::UnsupportedFidelity)),
        FunctionError::Operational(EvaluationError::UnsupportedCapability {
            capability: CapabilityId::Formatting,
            ..
        })
    ));
    let FunctionError::Operational(mut failure) =
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
        matches!(&failure, EvaluationError::ResourceLimit { target: Some(at), reason, .. } if *at == target && matches!(reason, crate::eval::limits::EvaluationReasonError::Format(error) if error.position == 17 && error.reason == FormatReasonError::FormatExceedsResourceLimit))
    );
    assert!(
        std::error::Error::source(&failure)
            .unwrap()
            .is::<crate::eval::limits::EvaluationReasonError>()
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
                Err(EvaluationError::Budget(_))
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
                CellValue::TextUtf16(crate::model::ExcelText::from("1,234.50%")),
                EvaluationBudgets::default()
            )
            .unwrap(),
            expected
        );
    }
}

#[test]
fn modern_calendar_continuation_tracks_only_consumed_context() {
    use crate::eval::ports::{ContextRead, DependencyRead, DependencyReadSink, ReadEvent};
    /// Collects context reads to verify date coercion depends only on settings it actually uses.
    #[derive(Default)]
    struct Reads(Vec<ReadEvent>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event);
        }
    }
    let mut w = Workbook::new();
    w.add_sheet("S").unwrap();
    for (text, order, expected, year_read, order_read) in [
        ("1/2/2020", DateOrder::MonthDayYear, 43832., false, true),
        ("1/2/2020", DateOrder::DayMonthYear, 43862., false, true),
        ("2020-01-02", DateOrder::DayMonthYear, 43832., false, false),
        ("1/2", DateOrder::MonthDayYear, 44198., true, true),
        ("12:00", DateOrder::DayMonthYear, 0.5, false, false),
    ] {
        let mut reads = Reads::default();
        let context = EvaluationInputs {
            as_of_time: UNIX_EPOCH + Duration::from_secs(1609502400),
            localization: Localization {
                utc_offset_seconds: 0,
                date_order: order,
            },
            ..Default::default()
        };
        let mut session = Session::with_context(&w, context);

        session.ports.dependency_sink = Some(&mut reads);
        assert_eq!(
            Coercer::new(&mut session)
                .coerce_numeric_date_time(text)
                .unwrap(),
            expected
        );
        drop(session);
        for (field, expected) in [
            (ContextRead::AsOf, year_read),
            (ContextRead::UtcOffset, year_read),
            (ContextRead::DateOrder, order_read),
        ] {
            assert_eq!(
                reads
                    .0
                    .iter()
                    .any(|r| r.read == DependencyRead::ContextField(field)),
                expected,
                "{text}: {field:?}"
            );
        }
    }
}

#[test]
fn impossible_calendar_text_rejects_before_acquiring_default_year() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    let mut session = Session::with_context(
        &workbook,
        EvaluationInputs {
            as_of_time: UNIX_EPOCH + Duration::from_secs(400000000000),
            ..Default::default()
        },
    );

    for input in ["hello-world", "2/30", "13/1"] {
        assert!(
            matches!(
                Coercer::new(&mut session).coerce_numeric_date_time(input),
                Err(FunctionError::Spreadsheet(CellError::Value))
            ),
            "{input}"
        );
    }
    // February 29 can be valid, so this request really needs the unavailable year.
    assert!(matches!(
        Coercer::new(&mut session).coerce_numeric_date_time("2/29"),
        Err(FunctionError::Operational(
            EvaluationError::UnsupportedCapability { .. }
        ))
    ));
}
