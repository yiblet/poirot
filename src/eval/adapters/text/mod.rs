//! Text kernels keep algorithms independent from typed evaluator execution.
use crate::eval::host::CallHost;
use crate::eval::*;
mod modern;
mod regex;
mod units;
pub(in crate::eval) use modern::execute as execute_modern;
pub(in crate::eval) use regex::execute as execute_regex;
pub(in crate::eval) use units::execute as execute_units;
/// Evaluator-owned text builtin dispatch.
pub(in crate::eval) mod execution;

/// Restore semantic failures as cells while preserving operational aborts.
fn finish(result: FunctionResult<Value>) -> EvalResult<Value> {
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}

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
        session.select_legacy_scalar(at, value)
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
            Text("a".into())
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
        /// Records CONCAT dependency reads so the test can verify traversal stops at the first consumed error.
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
        assert_eq!(session.select_legacy_scalar(at, value).unwrap(), Error(NA));
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
                text_bytes: 6000,
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
        assert_eq!(
            session.select_legacy_scalar(at, value).unwrap(),
            Text("x".repeat(3000))
        );
    }
    #[test]
    fn concat_keeps_semantic_limits_and_operational_budgets_separate() {
        let book = book();
        assert_eq!(
            eval(&book, "Z100", "CONCAT(REPT(\"😀\",16383))").unwrap(),
            Text("😀".repeat(16383))
        );
        assert_eq!(
            eval(&book, "Z100", "CONCAT(REPT(\"😀\",16383),\"x\")").unwrap(),
            Text(format!("{}x", "😀".repeat(16383)))
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
