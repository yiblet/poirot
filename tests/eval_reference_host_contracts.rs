//! Reference selection contracts from Microsoft's INDEX, ROW, and COLUMN documentation:
//! https://support.microsoft.com/en-us/excel/functions/index-function
//! https://support.microsoft.com/en-us/excel/functions/row-function
//! https://support.microsoft.com/en-us/excel/functions/column-function
use poirot::eval::{EvaluationBudgets, EvaluationError, EvaluationOutcome, run_formula};
use poirot::model::{CellError, CellValue, Workbook};

#[test]
fn index_metadata_and_sparse_selection_preserve_reference_results() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet.set_formula("A1", "UNKNOWN_FUNCTION()").unwrap();
    sheet.set_value("B4", 7.).unwrap();
    sheet.set_value("B1000000", 11.).unwrap();
    for (formula, expected) in [
        ("ROW(INDEX(A1:A3,2))", 2.),
        ("COLUMN(INDEX(A1:B3,1,2))", 2.),
        ("ROWS(INDEX(A:A,0))", 1_048_576.),
        ("COLUMNS(INDEX(A1:B3,0,0))", 2.),
        ("SUM(INDEX(B:B,0))", 18.),
    ] {
        assert!(
            matches!(
                run_formula(&workbook, 0, "Z1".parse().unwrap(), formula,
                    Default::default(), EvaluationBudgets {materialized_values: 4, ..Default::default()}),
                EvaluationOutcome::Complete {value: CellValue::Number(value), ..} if value == expected
            ),
            "{formula}"
        );
    }
}

#[test]
fn index_operational_failures_survive_semantic_errors_and_admission() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    let at = "Z1".parse().unwrap();
    for formula in [
        "INDEX(A1:B3,1/0,INDIRECT(\"[missing.xlsx]S!A1\"))",
        "INDEX(1/0,1,INDIRECT(\"[missing.xlsx]S!A1\"))",
    ] {
        assert!(
            matches!(
                run_formula(
                    &workbook,
                    0,
                    at,
                    formula,
                    Default::default(),
                    Default::default()
                ),
                EvaluationOutcome::Aborted {
                    failure: EvaluationError::UnsupportedCapability { .. },
                    ..
                }
            ),
            "{formula}"
        );
    }
    assert!(matches!(
        run_formula(
            &workbook,
            0,
            at,
            "INDEX(A1:B3,4,1)",
            Default::default(),
            Default::default()
        ),
        EvaluationOutcome::Complete {
            value: CellValue::Error(CellError::Ref),
            ..
        }
    ));
    assert!(matches!(
        run_formula(
            &workbook,
            0,
            at,
            "INDEX(A1:B3,2,2)",
            Default::default(),
            EvaluationBudgets {
                materialized_values: 0,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationError::Budget(_),
            ..
        }
    ));
}

#[test]
fn index_observes_arguments_in_source_order_and_completes_failed_frames() {
    use poirot::eval::{observe::*, *};
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    let prepared = compiler.compile_formula("INDEX(A1:B3,1/0,SQRT(-1),1)", &policy);
    let mut trace = BoundedExecutionTrace::new(RetentionLimits {
        events: 100,
        scalar_values: 100,
        text_bytes: 100,
    });
    let mut run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        Default::default(),
        Default::default(),
    )
    .unwrap()
    .with_execution_observer(&mut trace);
    let result = run.evaluate_formula(
        EvaluationTarget {
            sheet: 0,
            address: "Z1".parse().unwrap(),
        },
        &prepared,
    );
    assert_eq!(result.as_ref().unwrap(), &CellValue::Error(CellError::Num));
    let _ = run.finish(result);
    let root = trace
        .events()
        .iter()
        .find_map(|event| match event {
            RetainedEvent::Begin(operation) if operation.parent.is_none() => Some(operation.id),
            _ => None,
        })
        .unwrap();
    let ordinals: Vec<_> = trace
        .events()
        .iter()
        .filter_map(|event| match event {
            RetainedEvent::Begin(Operation {
                parent,
                kind: OperationKind::Argument { ordinal, .. },
                ..
            }) if *parent == Some(root) => Some(*ordinal),
            _ => None,
        })
        .collect();
    assert_eq!(ordinals, [0, 1, 2, 3]);
    for event in trace.events() {
        if let RetainedEvent::Begin(operation) = event {
            assert!(trace.events().iter().any(|event| matches!(event,
                RetainedEvent::Complete { operation: completed, .. } if completed.id == operation.id)));
        }
    }
}
