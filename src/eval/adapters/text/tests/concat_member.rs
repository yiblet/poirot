use crate::eval::compile;
use crate::eval::compile::bound::Expression;
use crate::eval::config::EvaluationInputs;
use crate::eval::limits::{BudgetMeter, EvalResult, EvaluationBudgets, EvaluationFailure};
use crate::eval::ports::{DependencyRead, DependencyReadSink, ReadEvent};
use crate::eval::session::{ResultProjection, Session};
use crate::eval::values::VALUE_ERROR;
use crate::eval::vm::ExpressionRunner;
use crate::model::CellError::NA;
use crate::model::CellValue::{Blank, Boolean, Error, Number, Text};
use crate::model::{CellAddress, CellValue, Workbook};
use poirot_parsing::formula::parse_formula;

fn expression(formula: &str) -> Expression {
    compile::bound::bind(&parse_formula(formula).unwrap())
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
    let value = ExpressionRunner::new(&mut session).expr(0, at, &expression(formula))?;
    ResultProjection::new(&mut session).select_legacy_scalar(at, value)
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
fn sparse_references_keep_row_major_order_and_blank_separators() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    for (at, text) in [("D1", "x"), ("E1", "y"), ("D5", "z")] {
        book.sheet_mut(0)
            .unwrap()
            .set_value(at.parse().unwrap(), Text(text.into()))
            .unwrap();
    }
    // Microsoft TEXTJOIN: FALSE keeps empty cells, delimiter ranges repeat,
    // and results over 32767 characters are #VALUE!.
    // https://support.microsoft.com/en-gb/excel/functions/textjoin-function
    for (formula, expected) in [
        ("CONCAT(D:E)", Text("xyz".into())),
        ("TEXTJOIN(\"-\",TRUE,D:E)", Text("x-y-z".into())),
        ("TEXTJOIN(\"-\",FALSE,D1:D4)", Text("x---".into())),
        ("TEXTJOIN(\"-\",FALSE,D2:D5)", Text("---z".into())),
        ("TEXTJOIN({\"1\",\"2\"},FALSE,D1:D5)", Text("x1212z".into())),
        ("TEXTJOIN(\"\",FALSE,D:D)", Text("xz".into())),
        ("TEXTJOIN(\"-\",FALSE,D:D)", Error(VALUE_ERROR)),
    ] {
        assert_eq!(eval(&book, "Z100", formula).unwrap(), expected, "{formula}");
    }
    // Dense array members follow the same separator rules as absent cells.
    assert_eq!(
        eval(
            &book,
            "Z100",
            "TEXTJOIN({\"1\",\"2\"},FALSE,{\"x\",\"\",\"\",\"\",\"z\"})"
        )
        .unwrap(),
        Text("x1212z".into())
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
    session.ports.dependency_sink = Some(&mut sink);
    let at = "Z100".parse().unwrap();
    let value = ExpressionRunner::new(&mut session)
        .expr(0, at, &expression("CONCAT(A1:B3,UNKNOWN_FUNCTION())"))
        .unwrap();
    assert_eq!(
        ResultProjection::new(&mut session)
            .select_legacy_scalar(at, value)
            .unwrap(),
        Error(NA)
    );
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
    let value = ExpressionRunner::new(&mut session)
        .expr(0, at, &expression("CONCAT(A1:A3000)"))
        .unwrap();
    assert_eq!(
        ResultProjection::new(&mut session)
            .select_legacy_scalar(at, value)
            .unwrap(),
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
                ExpressionRunner::new(&mut session).expr(
                    0,
                    "Z100".parse().unwrap(),
                    &expression(formula)
                ),
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
