//! Deferred POI text adapter. Number rendering uses the pinned converter;
//! PROPER uses the helper's explicit modern Unicode policy (Java-version drift
//! remains a documented compatibility limitation).
use crate::adapters::failure::{operational_only, project_host};
use crate::adapters::text::policy::{AreaSelection, Failure, Function, Host};
use crate::host::{CallHost, MissingPolicy, NumericPolicy, ScalarOperand};
use crate::*;
use std::cell::RefCell;

mod units;
/// UTF-16-preserving text execution path.
pub(crate) use units::execute as execute_units;

/// Text-adapter failure channel separating formula errors from operational refusals.
type TextFailure = Failure<CellError, EvaluationFailure>;
fn failure(failure: FunctionFailure) -> TextFailure {
    project_host(failure, Failure::Spreadsheet, Failure::Operational)
}
/// Deferred text host preserving operational failures across helper callbacks.
struct TextHost<'a, 'b> {
    host: RefCell<CallHost<'a, 'b>>,
    pending: RefCell<Option<FunctionFailure>>,
}
impl TextHost<'_, '_> {
    fn source_fault(&self, detail: &str) -> TextFailure {
        match self.host.try_borrow() {
            Ok(host) => {
                Failure::Operational(host.capability_failure(CapabilityId::FunctionAdapter, detail))
            }
            Err(_) => Failure::Operational(EvaluationFailure::StalePlan),
        }
    }
    fn coercev2_text_scalar(
        &self,
        scalar: &ScalarOperand,
        delimiter: bool,
    ) -> Result<String, TextFailure> {
        use poirot_coercionv2::compatibility::{DeferredText, ScalarRef};
        let input = if scalar.missing {
            ScalarRef::Missing
        } else {
            (&scalar.value).into()
        };
        poirot_coercionv2::compatibility::coercev2_text_argument(input, delimiter, |request| {
            match request {
                DeferredText::Missing => {
                    Err(self.source_fault("POI string conversion rejects MissingArgEval"))
                }
                DeferredText::Number(n) => self
                    .host
                    .try_borrow_mut()
                    .map_err(|_| Failure::Operational(EvaluationFailure::StalePlan))?
                    .coercev2_number_to_text(n, NumericPolicy::PinnedPoi)
                    .map_err(failure),
                DeferredText::Text(poirot_model::TextRef::Utf8(t)) => Ok(t.into()),
                DeferredText::Text(t) => self
                    .host
                    .try_borrow_mut()
                    .map_err(|_| Failure::Operational(EvaluationFailure::StalePlan))?
                    .project_text(t)
                    .map_err(failure),
            }
        })
    }
}
impl Host<usize> for TextHost<'_, '_> {
    /// Scalar projection exposed by this text host.
    type Scalar = ScalarOperand;
    /// Spreadsheet error identity exposed by this text host.
    type Error = CellError;
    /// Host abort channel kept outside catchable spreadsheet values.
    type Operational = EvaluationFailure;
    fn check_text(&mut self, bytes: usize) -> Result<(), EvaluationFailure> {
        self.host
            .get_mut()
            .check_text(bytes)
            .map_err(operational_only)
    }
    fn check_collection(&mut self, count: usize) -> Result<(), EvaluationFailure> {
        self.host
            .get_mut()
            .check_values(count)
            .map_err(operational_only)
    }
    fn charge_work(&mut self, units: usize) -> Result<(), EvaluationFailure> {
        self.host
            .get_mut()
            .charge_work(units as u64)
            .map_err(operational_only)
    }
    fn error(&self, error: poirot_functions_text::Error) -> CellError {
        match error {
            poirot_functions_text::Error::Value => VALUE_ERROR,
            poirot_functions_text::Error::Num => Num,
        }
    }
    fn is_missing(&self, scalar: &ScalarOperand) -> bool {
        scalar.missing
    }
    fn read_number_locale(&mut self) {
        self.host.get_mut().read_number_locale();
    }
    fn single(&mut self, index: &usize) -> Result<ScalarOperand, TextFailure> {
        let scalar = self
            .host
            .get_mut()
            .scalar_data(*index, MissingPolicy::Blank)
            .map_err(failure)?;
        if let Error(error) = scalar.value {
            return Err(Failure::Spreadsheet(error));
        }
        Ok(scalar)
    }
    fn values_bound(&self, index: &usize, selection: AreaSelection) -> Option<usize> {
        let description = self.host.try_borrow_mut().ok()?.describe(*index);
        match description {
            Ok(description) if !description.is_area => Some(1),
            Ok(description) => {
                description
                    .columns
                    .checked_mul(if selection == AreaSelection::LastRow {
                        1
                    } else {
                        description.rows
                    })
            }
            Err(error) => {
                *self.pending.try_borrow_mut().ok()? = Some(error);
                Some(0)
            }
        }
    }
    fn string_bound(&self, scalar: &ScalarOperand) -> Option<usize> {
        Some(match &scalar.value {
            Text(text) => text.len(),
            CellValue::TextUtf16(text) => text.len().checked_mul(3)?,
            Number(_) => 512,
            Boolean(_) => 5,
            _ => 0,
        })
    }
    fn values(
        &mut self,
        index: &usize,
        selection: AreaSelection,
    ) -> Result<Vec<ScalarOperand>, TextFailure> {
        if let Some(error) = self.pending.get_mut().take() {
            return Err(failure(error));
        }
        let description = self.host.get_mut().describe(*index).map_err(failure)?;
        if !description.is_area {
            return Ok(vec![self.single(index)?]);
        }
        let start_row = if selection == AreaSelection::LastRow {
            description.rows.saturating_sub(1)
        } else {
            0
        };
        let mut values = Vec::new();
        for row in start_row..description.rows {
            for column in 0..description.columns {
                let scalar = self
                    .host
                    .get_mut()
                    .read_cell(*index, row, column)
                    .map_err(failure)?;
                if let Error(error) = scalar.value {
                    return Err(Failure::Spreadsheet(error));
                }
                values.push(scalar);
            }
        }
        Ok(values)
    }
    fn coercev2_text_string(&self, scalar: &ScalarOperand) -> Result<String, TextFailure> {
        self.coercev2_text_scalar(scalar, false)
    }
    fn coercev2_text_delimiter(&self, scalar: &ScalarOperand) -> Result<String, TextFailure> {
        self.coercev2_text_scalar(scalar, true)
    }
    fn coercev2_text_integer(&self, scalar: &ScalarOperand) -> Result<i32, TextFailure> {
        if scalar.missing {
            return Err(self.source_fault("POI integer coercion rejects MissingArgEval"));
        }
        let number = match &scalar.value {
            Text(text) => self
                .host
                .try_borrow_mut()
                .map_err(|_| Failure::Operational(EvaluationFailure::StalePlan))?
                .coercev2_number_text(text, NumericPolicy::PinnedPoi)
                .map_err(failure)?,
            value => self
                .host
                .try_borrow_mut()
                .map_err(|_| Failure::Operational(EvaluationFailure::StalePlan))?
                .coercev2_scalar_number(value, NumericPolicy::PinnedPoi)
                .map_err(failure)?,
        };
        poirot_coercionv2::compatibility::coercev2_integer(number, true)
            .map_err(Failure::Spreadsheet)
    }

    fn coercev2_arabic_integer(&self, scalar: &ScalarOperand) -> Result<i32, TextFailure> {
        use poirot_coercionv2::{
            compatibility::ScalarRef,
            roman::{coercev2_arabic_scalar, coercev2_arabic_text},
        };
        let input = if scalar.missing {
            ScalarRef::Missing
        } else {
            (&scalar.value).into()
        };
        let text = coercev2_arabic_scalar(input).map_err(Failure::Spreadsheet)?;
        match text {
            poirot_model::TextRef::Utf8(text) => {
                coercev2_arabic_text(text).map_err(Failure::Spreadsheet)
            }
            text => {
                let text = self
                    .host
                    .try_borrow_mut()
                    .map_err(|_| Failure::Operational(EvaluationFailure::StalePlan))?
                    .project_text(text)
                    .map_err(failure)?;
                coercev2_arabic_text(&text).map_err(Failure::Spreadsheet)
            }
        }
    }
    fn boolean_value(&self, scalar: &ScalarOperand) -> Option<bool> {
        match scalar.value {
            Boolean(value) if !scalar.missing => Some(value),
            _ => None,
        }
    }
    fn coercev2_text_boolean(&self, scalar: &ScalarOperand) -> Result<bool, TextFailure> {
        if scalar.missing {
            return Err(self.source_fault("POI boolean conversion rejects MissingArgEval"));
        }
        if matches!(&scalar.value, Blank) {
            return Err(self.source_fault(
                "TEXTJOIN unboxes a null Boolean for a blank ignore_empty argument",
            ));
        }
        poirot_coercionv2::compatibility::coercev2_boolean(
            (&scalar.value).into(),
            poirot_coercionv2::compatibility::BooleanPolicy {
                reject_nan: true,
                blank: false,
            },
        )
        .map_err(Failure::Spreadsheet)?
        .ok_or(Failure::Spreadsheet(VALUE_ERROR))
    }
}

/// Execute a deferred text helper through the shared metered call host.
pub(crate) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: Function,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    match function {
        Function::NumberValue if !(1..=3).contains(&arguments.len()) => {
            return Ok(Number(0.).into());
        }
        Function::Proper if arguments.len() != 1 => return Ok(err(VALUE_ERROR)),
        Function::Arabic if arguments.len() != 1 => return Ok(err(VALUE_ERROR)),
        Function::Roman
            if !poirot_coercionv2::roman::coercev2_roman_arity(
                arguments.len(),
                session.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation,
            ) =>
        {
            return Ok(err(VALUE_ERROR));
        }
        Function::TextJoin if !(3..=254).contains(&arguments.len()) => return Ok(err(VALUE_ERROR)),
        _ => {}
    }
    let semantics = if session.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation {
        crate::adapters::text::policy::SemanticsPolicy::ExcelDocumentation
    } else {
        crate::adapters::text::policy::SemanticsPolicy::PinnedPoi
    };
    let policy = crate::adapters::text::policy::Policy {
        number_locale: session.number_locale,
        ..Default::default()
    };
    let mut host = TextHost {
        host: RefCell::new(CallHost::new(session, sheet, address, arguments)),
        pending: RefCell::new(None),
    };
    // Check before allocating the index-only call; no AST copies are made.
    host.check_collection(arguments.len())?;
    let call = crate::adapters::text::policy::Call {
        function,
        arguments: (0..arguments.len()).collect(),
    };
    match crate::adapters::text::policy::evaluate_with_policy(&call, &policy, &mut host, semantics)
    {
        Ok(crate::adapters::text::policy::Value::Number(value)) => Ok(Number(value).into()),
        Ok(crate::adapters::text::policy::Value::Text(value)) => Ok(Text(value).into()),
        Err(Failure::Spreadsheet(error)) => Ok(err(error)),
        Err(Failure::Operational(error)) => Err(error),
        Err(other) => Err(host.host.get_mut().capability_failure(
            CapabilityId::FunctionAdapter,
            format!("pinned text source fault or unsupported bound: {other:?}"),
        )),
    }
}

#[cfg(test)]
mod semantics_port_tests {
    use super::*;
    use poirot_parsing::formula::parse_formula;
    fn eval(workbook: &Workbook, at: &str, formula: &str) -> EvalResult<CellValue> {
        let mut session = Session::with_context(workbook, EvaluationInputs::default());
        session.text_policy = TextSemanticsPolicy {
            source: TextSourceVersion::Poi7321209,
        };
        let address = at.parse().unwrap();
        let value = session.expr(0, address, &bound::bind(&parse_formula(formula).unwrap()))?;
        session.scalar(address, value)
    }
    fn book() -> Workbook {
        let mut workbook = Workbook::new();
        workbook.add_sheet("Sheet1").unwrap();
        for (address, value) in [("A1", "a"), ("B1", "b"), ("A2", "c"), ("B2", "d")] {
            workbook
                .sheet_mut(0)
                .unwrap()
                .set_value(address.parse().unwrap(), value)
                .unwrap();
        }
        workbook
    }
    #[test]
    fn concat_traverses_dense_area_and_array_without_scalar_intersection() {
        let workbook = book();
        for formula in ["CONCAT(A1:B2)", "CONCAT({\"a\",\"b\";\"c\",\"d\"})"] {
            assert_eq!(
                eval(&workbook, "Z20", formula).unwrap(),
                Text("abcd".into())
            );
        }
        assert_eq!(
            eval(&workbook, "B1", "CONCATENATE(A1:B1)").unwrap(),
            Text("b".into())
        );
        assert_eq!(
            eval(&workbook, "Z20", "CONCATENATE(A1:B2)").unwrap(),
            Error(VALUE_ERROR)
        );
        assert_eq!(
            eval(&workbook, "Z20", "CONCAT(C1:C2,\"x\")").unwrap(),
            Text("x".into())
        );
        assert_eq!(
            eval(&workbook, "Z20", "CONCAT()").unwrap(),
            Text(String::new())
        );
    }
    #[test]
    fn missing_and_selected_errors_preserve_failure_domain_and_source_order() {
        let mut workbook = book();
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value("A2".parse().unwrap(), Error(NA))
            .unwrap();
        assert_eq!(eval(&workbook, "Z20", "CONCAT(A1:B2)").unwrap(), Error(NA));
        assert_eq!(
            eval(&workbook, "A1", "CONCATENATE(A1:A2)").unwrap(),
            Text("a".into())
        );
        assert_eq!(
            eval(&workbook, "A2", "CONCATENATE(A1:A2)").unwrap(),
            Error(NA)
        );
        for formula in [
            "CONCAT(,\"x\")",
            "CONCATENATE(,\"x\")",
            "LEN()",
            "LEFT(\"x\",)",
            "MID(\"x\",,1)",
        ] {
            if formula == "LEN()" {
                assert_eq!(eval(&workbook, "Z20", formula).unwrap(), Error(VALUE_ERROR));
            } else {
                assert!(
                    matches!(
                        eval(&workbook, "Z20", formula),
                        Err(EvaluationFailure::UnsupportedCapability { .. })
                    ),
                    "{formula}"
                );
            }
        }
        assert_eq!(
            eval(&workbook, "Z20", "LEFT(\"abc\",2147483648)").unwrap(),
            Error(Num)
        );
        // The first source coercion fails before a later error can mask it.
        assert!(matches!(
            eval(&workbook, "Z20", "CONCATENATE(,#N/A)"),
            Err(EvaluationFailure::UnsupportedCapability { .. })
        ));
        assert_eq!(
            eval(&workbook, "Z20", "CONCATENATE(#N/A,)").unwrap(),
            Error(NA)
        );
    }
    #[test]
    fn dense_concat_does_not_swallow_operational_budget_abort() {
        let workbook = book();
        let mut session = Session::new(
            &workbook,
            EvaluationInputs::default(),
            BudgetMeter::new(EvaluationBudgets {
                materialized_values: 2,
                ..EvaluationBudgets::default()
            })
            .unwrap(),
        );
        session.text_policy = TextSemanticsPolicy {
            source: TextSourceVersion::Poi7321209,
        };
        let result = session.expr(
            0,
            "Z20".parse().unwrap(),
            &bound::bind(&parse_formula("CONCAT(A1:B2)").unwrap()),
        );
        assert!(matches!(result, Err(EvaluationFailure::Budget(_))));
    }
}

/// Deferred text operation contracts and compatibility policies.
pub mod policy;

/// Evaluator-owned text builtin dispatch.
pub(crate) mod execution;

#[cfg(test)]
mod concat_member_tests {
    use super::*;
    use poirot_parsing::formula::parse_formula;

    fn expression(formula: &str) -> Expr {
        bound::bind(&parse_formula(formula).unwrap())
    }
    fn book() -> Workbook {
        let mut book = Workbook::new();
        book.add_sheet("S").unwrap();
        for (at, value) in [
            ("A1", Text("a".into())),
            ("B1", Boolean(true)),
            ("A2", Blank),
            ("B2", Number(2.)),
            ("A3", Error(NA)),
            ("B3", Error(VALUE_ERROR)),
        ] {
            book.sheet_mut(0)
                .unwrap()
                .set_value(at.parse().unwrap(), value)
                .unwrap();
        }
        book
    }
    fn eval(book: &Workbook, at: &str, formula: &str) -> EvalResult<CellValue> {
        let mut session = Session::with_context(book, EvaluationInputs::default());
        let at = at.parse().unwrap();
        let value = session.expr(0, at, &expression(formula))?;
        session.scalar(at, value)
    }
    #[test]
    fn concat_traverses_argument_members_in_row_major_order() {
        let book = book();
        for (formula, expected) in [
            (
                "CONCAT({\"A\",\"B\",\"C\"},{0,1},{2;3},{4,5,6;7,8,9},\"Z\")",
                "ABC0123456789Z",
            ),
            ("CONCAT(\"<\",A1:B2,{\"😀\",\"é\"},\">\")", "<aTRUE2😀é>"),
            ("CONCAT({1,2}+{10;20})", "11122122"),
            ("CONCAT({\"a\",\"b\"}&{1;2})", "a1b1a2b2"),
            ("CONCAT(,C1,\"\",FALSE,0,\"z\")", "FALSE0z"),
            ("CONCAT(IF(TRUE,{1,2;3,4},#N/A))", "1234"),
        ] {
            assert_eq!(
                eval(&book, "Z100", formula).unwrap(),
                Text(expected.into()),
                "{formula}"
            );
        }
        // Collection traversal does not opt reference expressions into array demand.
        assert_eq!(
            eval(&book, "Z100", "CONCAT(A1:A2&\"x\")").unwrap(),
            Error(VALUE_ERROR)
        );
        assert_eq!(
            eval(&book, "B1", "CONCAT(A1:A2&\"x\")").unwrap(),
            Text("ax".into())
        );
        assert_eq!(
            eval(&book, "B1", "CONCATENATE(A1:B1)").unwrap(),
            Text("TRUE".into())
        );
        assert_eq!(
            eval(&book, "Z100", "CONCATENATE({1,2})").unwrap(),
            Text("1".into())
        );
        assert_eq!(
            eval(&book, "Z100", "TEXTJOIN(\"|\",TRUE,A1:B2)").unwrap(),
            Text("a|TRUE|2".into())
        );
    }
    #[test]
    fn concat_preserves_error_order_and_stops_dependency_reads() {
        let book = book();
        #[derive(Default)]
        struct Reads(Vec<ReadEvent>);
        impl DependencyReadSink for Reads {
            fn read(&mut self, event: ReadEvent) {
                self.0.push(event);
            }
        }
        let mut sink = Reads::default();
        let mut session = Session::with_context(&book, EvaluationInputs::default());
        session.dependency_sink = Some(&mut sink);
        let at = "Z100".parse().unwrap();
        let value = session
            .expr(0, at, &expression("CONCAT(A1:B3,UNKNOWN_FUNCTION())"))
            .unwrap();
        assert_eq!(session.scalar(at, value).unwrap(), Error(NA));
        drop(session);
        let addresses: Vec<_> = sink
            .0
            .iter()
            .filter_map(|event: &ReadEvent| match &event.read {
                DependencyRead::Cell(target) => Some(target.address),
                _ => None,
            })
            .collect();
        let expected: Vec<CellAddress> = ["A1", "B1", "A2", "B2", "A3"]
            .iter()
            .map(|a| a.parse().unwrap())
            .collect();
        assert_eq!(addresses, expected);
        assert_eq!(
            eval(&book, "Z100", "CONCAT({#VALUE!,#N/A},#DIV/0!)").unwrap(),
            Error(VALUE_ERROR)
        );
    }
    #[test]
    fn concat_streams_text_without_quadratic_work_reservations() {
        let mut book = Workbook::new();
        book.add_sheet("S").unwrap();
        for row in 0..3000 {
            book.sheet_mut(0)
                .unwrap()
                .set_value(CellAddress { row, col: 0 }, "x")
                .unwrap();
        }
        let mut session = Session::new(
            &book,
            EvaluationInputs::default(),
            BudgetMeter::new(EvaluationBudgets {
                text_bytes: 3000,
                run_work: 200_000,
                cell_work: 200_000,
                ..Default::default()
            })
            .unwrap(),
        );
        let at = "Z100".parse().unwrap();
        let value = session
            .expr(0, at, &expression("CONCAT(A1:A3000)"))
            .unwrap();
        assert_eq!(session.scalar(at, value).unwrap(), Text("x".repeat(3000)));
    }
    #[test]
    fn concat_keeps_semantic_limits_and_operational_budgets_separate() {
        let book = book();
        assert_eq!(
            eval(&book, "Z100", "CONCAT(REPT(\"😀\",32767))").unwrap(),
            Text("😀".repeat(32767))
        );
        assert_eq!(
            eval(&book, "Z100", "CONCAT(REPT(\"😀\",32767),\"x\")").unwrap(),
            Error(VALUE_ERROR)
        );
        for (limits, formula) in [
            (
                EvaluationBudgets {
                    text_bytes: 8,
                    ..Default::default()
                },
                "IFERROR(CONCAT(\"abcd\",\"efghi\"),\"caught\")",
            ),
            (
                EvaluationBudgets {
                    materialized_values: 2,
                    ..Default::default()
                },
                "IFERROR(CONCAT({1,2,3}),\"caught\")",
            ),
            (
                EvaluationBudgets {
                    cell_work: 10,
                    run_work: 10,
                    ..Default::default()
                },
                "IFERROR(CONCAT(A1:B2),\"caught\")",
            ),
        ] {
            let mut session = Session::new(
                &book,
                EvaluationInputs::default(),
                BudgetMeter::new(limits).unwrap(),
            );
            assert!(
                matches!(
                    session.expr(0, "Z100".parse().unwrap(), &expression(formula)),
                    Err(EvaluationFailure::Budget(_))
                ),
                "{formula}"
            );
        }
        // A large blank range streams without a member vector or quadratic text charges.
        assert_eq!(
            eval(&book, "Z100", "CONCAT(C1:C20000)").unwrap(),
            Text(String::new())
        );
    }
}
