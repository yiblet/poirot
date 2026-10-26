//! Aggregate collections preserve operand origins and ordered numerical reduction.
//! Expectations use Microsoft's SUM/COUNT/AND contracts: direct numeric text and
//! logical arguments may coerce, while text/logical members of references and
//! arrays are ignored by SUM/COUNT. No expected values derive from engine output.
//! https://support.microsoft.com/en-us/office/sum-function-043e1c7d-7726-4e80-8f32-07b23e057f89
//! https://support.microsoft.com/en-us/office/count-function-a59cd7fc-b623-4d93-87a4-d23bf411294c
//! https://support.microsoft.com/en-us/office/and-function-5f19b2e8-e1df-4408-897a-ce285a19e9d9
use poirot::eval::{EvaluationOutcome, run_formula};
use poirot::model::{CellError, CellValue, Workbook};

#[test]
fn eager_members_and_selected_origins_follow_aggregate_coercion() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1".parse().unwrap(), "3").unwrap();
    sheet.set_value("A2".parse().unwrap(), true).unwrap();
    sheet.set_value("A3".parse().unwrap(), 4.).unwrap();
    for (formula, expected) in [
        ("SUM(A1:A4)", CellValue::Number(4.)),
        ("SUM(\"3\",TRUE,4)", CellValue::Number(8.)),
        ("SUM({\"3\",TRUE,4})", CellValue::Number(4.)),
        ("COUNT(A1:A4)", CellValue::Number(1.)),
        ("COUNT(\"3\",TRUE,4)", CellValue::Number(3.)),
        ("COUNT({\"3\",TRUE,4})", CellValue::Number(1.)),
        ("SUM(INDEX(A1:A3,1))", CellValue::Number(0.)),
        ("SUM(INDEX({2,4},1,2))", CellValue::Number(4.)),
        ("AND(A1:A4)", CellValue::Boolean(true)),
        ("AND({TRUE,FALSE})", CellValue::Boolean(false)),
        ("SUM(A4:A8)", CellValue::Number(0.)),
        ("SUM({1,#REF!,#DIV/0!})", CellValue::Error(CellError::Ref)),
        // Ordinary ordered IEEE addition: ((1e16 + 1) - 1e16) + 2 = 2.
        ("SUM({1E16,1,-1E16,2})", CellValue::Number(2.)),
    ] {
        let EvaluationOutcome::Complete { value, .. } = run_formula(
            &book,
            0,
            "Z1".parse().unwrap(),
            formula,
            Default::default(),
            Default::default(),
        ) else {
            panic!("unexpected operational failure: {formula}");
        };
        assert_eq!(value, expected, "{formula}");
    }
}

#[test]
fn compact_reference_members_preserve_empty_text_boolean_and_error_contracts() {
    // Microsoft COUNTA includes empty strings and errors; COUNT includes only
    // numeric reference members. COUNTBLANK includes empty strings and blanks.
    // https://support.microsoft.com/en-us/office/counta-function-7dc98875-d5c1-46f1-9a82-53f3219e2509
    // https://support.microsoft.com/en-us/office/countblank-function-6a92d772-675c-4bee-b346-24af6bd3ac22
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (address, value) in [
        ("A1", CellValue::Number(4.)),
        ("A2", CellValue::Text(String::new())),
        (
            "A3",
            CellValue::TextUtf16(poirot::model::ExcelText::from_units(vec![])),
        ),
        (
            "A4",
            CellValue::TextUtf16(poirot::model::ExcelText::from_units(vec![0xd800])),
        ),
        ("A5", CellValue::Boolean(true)),
        ("A6", CellValue::Error(CellError::Ref)),
    ] {
        sheet.set_value(address.parse().unwrap(), value).unwrap();
    }
    for (formula, expected) in [
        ("SUM(A1:A5)", CellValue::Number(4.)),
        ("AVERAGE(A1:A5)", CellValue::Number(4.)),
        ("MIN(A1:A5)", CellValue::Number(4.)),
        ("MAX(A1:A5)", CellValue::Number(4.)),
        ("COUNT(A1:A7)", CellValue::Number(1.)),
        ("COUNTA(A1:A7)", CellValue::Number(6.)),
        ("COUNTBLANK(A1:A7)", CellValue::Number(3.)),
        ("AND(A1:A5)", CellValue::Boolean(true)),
        ("OR(A1:A5)", CellValue::Boolean(true)),
        ("SUM(A1:A7)", CellValue::Error(CellError::Ref)),
        ("AND(A1:A7)", CellValue::Error(CellError::Ref)),
        ("SUM(INDEX(A1:A5,2))", CellValue::Number(0.)),
        ("COUNTA({1,TRUE,\"\",#REF!})", CellValue::Number(4.)),
    ] {
        let EvaluationOutcome::Complete { value, .. } = run_formula(
            &book,
            0,
            "Z1".parse().unwrap(),
            formula,
            Default::default(),
            Default::default(),
        ) else {
            panic!("unexpected operational failure: {formula}");
        };
        assert_eq!(value, expected, "{formula}");
    }
}

#[test]
fn eager_reference_projection_does_not_hide_a_later_operational_failure() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_value("A1".parse().unwrap(), CellValue::Error(CellError::Div0))
        .unwrap();
    // A reversed sheet span is an unsupported capability, distinct from an
    // Excel error. Eager references must finish this body before coercion.
    sheet
        .set_formula("A2".parse().unwrap(), "SUM(T:S!B1)")
        .unwrap();
    book.add_sheet("T").unwrap();
    assert!(matches!(
        run_formula(
            &book,
            0,
            "Z1".parse().unwrap(),
            "SUM(A1:A2)",
            Default::default(),
            Default::default(),
        ),
        EvaluationOutcome::Aborted {
            failure: poirot::eval::EvaluationFailure::UnsupportedCapability {
                capability: poirot::eval::CapabilityId::FunctionAdapter,
                ..
            },
            ..
        }
    ));
}

/// Capture complete values before aggregate projection discards text payloads.
#[derive(Default)]
struct AggregateObservations(
    Vec<(
        poirot::eval::EvaluationTarget,
        Result<CellValue, poirot::eval::EvaluationFailure>,
    )>,
);
impl poirot::eval::CellObserver for AggregateObservations {
    fn completed(
        &mut self,
        event: poirot::eval::CellCompletion<'_>,
    ) -> Result<(), poirot::eval::EvaluationFailure> {
        self.0
            .push((event.target, event.result.cloned().map_err(Clone::clone)));
        Ok(())
    }
}

#[test]
fn ignored_reference_text_still_has_full_completion_values_and_text_admission() {
    use poirot::eval::*;
    let at = |address: &str| EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    };
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_value(
            at("A1").address,
            CellValue::TextUtf16(poirot::model::ExcelText::from_units(vec![0xd800; 16])),
        )
        .unwrap();
    sheet.set_value(at("A2").address, 7.).unwrap();
    sheet.set_formula(at("Z1").address, "SUM(A1:A2)").unwrap();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
    // The finite cap admits the 10-byte formula but rejects the 32-byte text payload.
    for text_bytes in [usize::MAX, 16] {
        let mut observed = AggregateObservations::default();
        let mut run = EvaluationRun::new(
            WorkbookView::new(&book, &plan, &policy).unwrap(),
            Default::default(),
            EvaluationBudgets {
                text_bytes,
                ..Default::default()
            },
        )
        .unwrap()
        .with_cell_observer(&mut observed);
        let result = run.evaluate_cell(at("Z1"));
        let report = run.finish(result.clone());
        if text_bytes == 16 {
            assert!(
                matches!(result, Err(EvaluationFailure::Budget(BudgetFailure {
                kind: BudgetKind::TextBytes, target: Some(target), ..
            })) if target == at("A1"))
            );
            assert_eq!(
                observed
                    .0
                    .iter()
                    .map(|(target, _)| *target)
                    .collect::<Vec<_>>(),
                vec![at("A1"), at("Z1")]
            );
        } else {
            assert_eq!(result, Ok(CellValue::Number(7.)));
            assert_eq!(
                observed.0,
                vec![
                    (
                        at("A1"),
                        Ok(CellValue::TextUtf16(poirot::model::ExcelText::from_units(
                            vec![0xd800; 16]
                        )))
                    ),
                    (at("A2"), Ok(CellValue::Number(7.))),
                    (at("Z1"), Ok(CellValue::Number(7.))),
                ]
            );
            assert_eq!(report.outcome.usage().cell_reads, 3);
        }
    }
}
