//! Source-derived semantic and operational cases. Retained native bit controls
//! remain in retained_time.rs; these assertions are not native golden evidence.
use super::super::text_coercion::coerce_time_text;
use crate::eval::compile;
use crate::eval::config::{DateOrder, EvaluationInputs, Localization};
use crate::eval::entry::run_plan;
use crate::eval::limits::{
    BudgetKind, BudgetMeter, EvaluationBudgets, EvaluationError, EvaluationOutcome,
};
use crate::eval::ports::DependencyRead;
use crate::eval::session::Session;
use crate::eval::values::{FunctionError, FunctionResult, VALUE_ERROR};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellError::Num;
use crate::model::CellValue::{Error, Number};
use crate::model::{CellValue, DateSystem, Workbook};
use poirot_functions::coercion::dates::calendar::Date;
use poirot_functions::coercion::dates::time;
use std::time::{Duration, UNIX_EPOCH};

fn workbook(system: DateSystem) -> Workbook {
    let mut book = Workbook::new();
    book.date_system = system;
    book.add_sheet("Sheet1").unwrap();
    book
}

fn context() -> EvaluationInputs {
    EvaluationInputs {
        as_of_time: UNIX_EPOCH + Duration::from_secs(1704067200),
        ..EvaluationInputs::default()
    }
}

fn parse_with_host(session: &mut Session<'_>, input: &str) -> FunctionResult<f64> {
    let mut host =
        crate::eval::host::CallHost::new(session, 0, "A1".parse().unwrap(), ArgumentsRef::Empty);
    coerce_time_text(&mut host, input)
}

fn dispatch(book: &Workbook, input: &str, context: EvaluationInputs) -> CellValue {
    match crate::eval::entry::run_formula(
        book,
        0,
        "A1".parse().unwrap(),
        &format!("TIMEVALUE(\"{input}\")"),
        context,
        Default::default(),
    ) {
        crate::eval::limits::EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("evaluation aborted: {outcome:?}"),
    }
}

#[test]
fn actual_dispatch_source_grammar_and_fallback_both_epochs() {
    for system in [DateSystem::Excel1900, DateSystem::Excel1904] {
        let book = workbook(system);
        for (input, expected) in [
            (
                "12:30 12:30",
                Number(
                    (time::source_day(Date {
                        y: 2000,
                        m: 1,
                        d: 1,
                    }) + 45000. / 86400.)
                        - time::source_day(Date {
                            y: 2000,
                            m: 1,
                            d: 1,
                        }),
                ),
            ),
            ("12:30 13:30", Error(VALUE_ERROR)),
            ("0:00 AM", Number(0.)),
            ("0:00 PM", Number(0.5)),
            ("12:00 AM", Number(0.)),
            ("12:00 PM", Number(0.5)),
            ("24:00:00.000", Number(1.)),
            ("24:00:00.001", Error(VALUE_ERROR)),
            ("24:01", Error(VALUE_ERROR)),
            ("25:00", Error(VALUE_ERROR)),
            ("2000-01-01 12:30:15", Number(0.)),
            ("1/1/2000 12:30:15", Number(0.)),
            ("2/31/2000 12:00", Error(VALUE_ERROR)),
            ("2/29/1900 12:00", Error(VALUE_ERROR)),
            ("0000-Jan-01", Number(0.)),
            ("1/01/2000 trailing text", Number(0.)),
        ] {
            assert_eq!(
                dispatch(&book, input, context()),
                expected,
                "{system:?}: {input}"
            );
        }
    }
}

#[test]
fn source_excess_day_uses_1900_serial_at_1899_boundary() {
    for system in [DateSystem::Excel1900, DateSystem::Excel1904] {
        let book = workbook(system);
        let original = Date {
            y: 1899,
            m: 12,
            d: 31,
        };
        let next = Date {
            y: 1900,
            m: 1,
            d: 1,
        };
        let expected = time::source_day(next) - time::source_day(original);
        assert_eq!(
            dispatch(&book, "12/31/1899 24:00", context()),
            Number(expected)
        );
    }
}

#[derive(Default)]
/// Records clock and localization fields consumed by TIMEVALUE dispatch.
struct Reads(Vec<DependencyRead>);

impl crate::eval::ports::DependencyReadSink for Reads {
    fn read(&mut self, event: crate::eval::ports::ReadEvent) {
        self.0.push(event.read);
    }
}

#[test]
fn context_year_is_explicit_lazy_and_invalid_context_propagates() {
    let book = workbook(DateSystem::Excel1900);
    for (input, expects_clock) in [("1/01/2000 12:00", false), ("bogus", false), ("2/29", true)] {
        let mut reads = Reads::default();
        let mut session = Session::with_context(&book, context());
        session.ports.dependency_sink = Some(&mut reads);
        let _ = parse_with_host(&mut session, input);
        drop(session);
        let fields: Vec<_> = reads
            .0
            .into_iter()
            .filter_map(|read| match read {
                DependencyRead::ContextField(field) => Some(field),
                _ => None,
            })
            .collect();
        let expected = if expects_clock {
            vec![
                crate::eval::ports::ContextRead::AsOf,
                crate::eval::ports::ContextRead::UtcOffset,
            ]
        } else {
            vec![]
        };
        assert_eq!(fields, expected, "{input}");
    }
    let invalid = EvaluationInputs {
        as_of_time: UNIX_EPOCH + Duration::from_secs(300_000_000_000),
        ..context()
    };
    assert_eq!(dispatch(&book, "1/01/2000 12:00", invalid), Number(0.5));
    assert_eq!(dispatch(&book, "bogus", invalid), Error(VALUE_ERROR));
    assert_eq!(dispatch(&book, "2/29", invalid), Error(Num));
    assert_eq!(dispatch(&book, "2/29", context()), Number(0.));
    let prior_year = EvaluationInputs {
        as_of_time: UNIX_EPOCH + Duration::from_secs(1704067199),
        ..context()
    };
    assert_eq!(dispatch(&book, "2/29", prior_year), Error(VALUE_ERROR));
    let offset_year = EvaluationInputs {
        localization: Localization {
            utc_offset_seconds: 1,
            date_order: DateOrder::DayMonthYear,
        },
        ..prior_year
    };
    assert_eq!(dispatch(&book, "2/29", offset_year), Number(0.));
}

#[test]
fn each_allocating_host_stage_is_admitted_before_semantic_fallback() {
    let book = workbook(DateSystem::Excel1900);
    let input = "1/01/2000 bad";
    let n = input.len() as u64;
    let dummy = n + 10;
    // Calls the actual host helper so expression/coercion costs do not obscure
    // the three allocation boundaries. All failures remain operational.
    for (work, limit, consumed, requested, kind) in [
        (n - 1, 100usize, 0, n, BudgetKind::RunWork),
        (1000, input.len(), n, dummy, BudgetKind::TextBytes),
        (n + dummy - 1, 100, n, dummy, BudgetKind::RunWork),
        (
            n + dummy + dummy - 1,
            100,
            n + dummy,
            dummy,
            BudgetKind::RunWork,
        ),
    ] {
        let budgets = EvaluationBudgets {
            run_work: work,
            text_bytes: limit,
            ..EvaluationBudgets::default()
        };
        let mut session = Session::new(&book, context(), BudgetMeter::new(budgets).unwrap());
        let failure = parse_with_host(&mut session, input).unwrap_err();
        let FunctionError::Operational(EvaluationError::Budget(failure)) = failure else {
            panic!("allocation admission became semantic fallback");
        };
        assert_eq!(failure.kind, kind);
        assert_eq!(failure.requested, requested);
        assert_eq!(session.state.runtime.budget.usage.text_bytes, consumed);
    }
    let mut session = Session::with_context(&book, context());
    assert_eq!((parse_with_host(&mut session, input)).unwrap(), 0.);
    let formula = "IFERROR(TIMEVALUE(\"1/01/2000 bad\"),0)";
    let plan = compile::bound::FormulaPlan::bind(
        &poirot_parsing::formula::parse_formula(formula).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        run_plan(
            &book,
            0,
            "A1".parse().unwrap(),
            &plan,
            context(),
            EvaluationBudgets {
                text_bytes: input.len(),
                ..EvaluationBudgets::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationError::Budget(_),
            ..
        }
    ));
}
