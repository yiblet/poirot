//! Checked search execution retains public ordering, reference, and failure contracts.
//! First exact positions and reverse search follow Microsoft's lookup documentation:
//! https://support.microsoft.com/en-us/excel/functions/vlookup-function
//! https://support.microsoft.com/en-us/excel/functions/xlookup-function
//! Deferred classic operand order and operational failures are Poirot contracts.
use poirot::eval::{observe::*, overlay::Evaluation, overlay::Overlay, *};
use poirot::model::{CellError, CellValue, Workbook};

#[test]
fn classic_search_observation_preserves_deferred_result_and_mode_order() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    for (address, value) in [("A1", 1.), ("A2", 2.), ("B1", 10.), ("B2", 20.)] {
        sheet.set_value(address.parse().unwrap(), value).unwrap();
    }
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    for (formula, expected, expected_ordinals) in [
        ("MATCH(2,A1:A2,0)", CellValue::Number(2.), vec![2, 0]),
        (
            "MATCH(NA(),A1:A2,0)",
            CellValue::Error(CellError::NA),
            vec![2, 0],
        ),
        (
            "VLOOKUP(9,A1:B2,UNKNOWN_FUNCTION(),FALSE)",
            CellValue::Error(CellError::NA),
            vec![0, 3],
        ),
        (
            "VLOOKUP(2,A1:B2,2,FALSE)",
            CellValue::Number(20.),
            vec![0, 3, 2],
        ),
        (
            "HLOOKUP(10,A1:B2,2,FALSE)",
            CellValue::Number(20.),
            vec![0, 3, 2],
        ),
    ] {
        let prepared = compiler.compile_formula(formula, &policy);
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
        assert_eq!(result.as_ref().unwrap(), &expected, "{formula}");
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
        assert_eq!(ordinals, expected_ordinals, "{formula}");
    }
}

#[test]
fn checked_search_retains_duplicates_and_reference_results_in_direct_and_overlay_runs() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    for (address, value) in [("A1", 2.), ("A2", 2.), ("B1", 20.), ("B2", 21.)] {
        sheet.set_value(address.parse().unwrap(), value).unwrap();
    }
    for (address, formula) in [
        ("D1", "VLOOKUP(2,A:B,2,FALSE)"),
        ("D2", "MATCH(2,A:A,0)"),
        ("D3", "XLOOKUP(2,A1:A2,B1:B2,,,-1)"),
        ("D4", "XMATCH(2,A1:A2,0,-1)"),
        ("D5", "ROW(XLOOKUP(2,A1:A2,B1:B2))"),
        ("D6", "LOOKUP(2,A1:A2,B1:B2)"),
    ] {
        sheet
            .set_formula(address.parse().unwrap(), formula)
            .unwrap();
    }
    let mut direct = Evaluation::with_workbook(&workbook)
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
    for (address, expected) in [
        ("D1", 20.),
        ("D2", 1.),
        ("D3", 21.),
        ("D4", 2.),
        ("D5", 1.),
        ("D6", 21.),
    ] {
        let target = EvaluationTarget {
            sheet: 0,
            address: address.parse().unwrap(),
        };
        assert_eq!(
            direct.cell(target).unwrap(),
            CellValue::Number(expected),
            "{address}"
        );
        assert_eq!(
            overlay
                .evaluation(&Default::default())
                .unwrap()
                .cell(target)
                .unwrap(),
            CellValue::Number(expected),
            "{address}"
        );
    }
}

#[test]
fn search_work_failures_keep_caller_provenance_and_escape_iferror() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    let target = EvaluationTarget {
        sheet: 0,
        address: "Z9".parse().unwrap(),
    };
    for formula in [
        "IFERROR(VLOOKUP(2,{1,10;2,20},2,FALSE),99)",
        "IFERROR(HLOOKUP(2,{1,2;10,20},2,FALSE),99)",
        "IFERROR(MATCH(2,{1;2},0),99)",
        "IFERROR(XLOOKUP(2,{1;2},{10;20}),99)",
        "IFERROR(XMATCH(2,{1;2}),99)",
        "IFERROR(LOOKUP(2,{1;2},{10;20}),99)",
    ] {
        assert!(
            matches!(run_formula(&workbook, target.sheet, target.address, formula,
            Default::default(), EvaluationBudgets {run_work: 5, ..Default::default()}),
            EvaluationOutcome::Aborted {failure: EvaluationFailure::Budget(BudgetFailure {target: Some(at), ..}), ..} if at == target),
            "{formula}"
        );
    }
}
