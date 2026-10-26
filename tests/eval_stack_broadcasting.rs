//! Generic stack operations follow singleton-axis broadcasting and Excel's lazy IF rules.
use poirot::eval::{
    ArrayResultPolicy, EvaluationRun, EvaluationTarget, FormulaCompiler, PreparationPolicy,
    PreparedFormula, RunConfiguration, WorkbookView,
    overlay::{EvaluationContext, EvaluationPolicy, Overlay},
};
use poirot::model::{CellError, CellValue, Workbook};

fn target(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}

#[test]
fn reference_plus_scalar_spills_each_result_and_tracks_edits() {
    // Microsoft documents returning an array to neighboring cells as spilling:
    // https://support.microsoft.com/en-us/office/dynamic-array-formulas-and-spilled-array-behavior-205c6b06-03ba-4151-89a1-87a7eb36e531
    // These expected values are ordinary memberwise arithmetic, not captured engine output.
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, value) in [("A1", 10.), ("A2", 20.), ("A3", 30.)] {
        sheet.set_value(target(cell).address, value).unwrap();
    }
    sheet.set_formula(target("C1").address, "A1:A3+1").unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
            arrays: ArrayResultPolicy::DynamicSpill,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    for (cell, expected) in [("C1", 11.), ("C2", 21.), ("C3", 31.)] {
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target(cell))
                .unwrap(),
            CellValue::Number(expected)
        );
    }
    overlay.set_value(target("A2"), 40.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C2"))
            .unwrap(),
        CellValue::Number(41.)
    );
}

#[test]
fn one_prepared_body_handles_changing_condition_shapes_without_recompilation() {
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let prepared = compiler.compile_formula(
        "SUM(IF(IF(A1,TRUE,{TRUE;FALSE;TRUE}),{1;2;3},10)+B1)",
        &policy,
    );
    let PreparedFormula::Ready(formula) = prepared.as_ref() else {
        panic!("valid formula")
    };
    let original = formula.bytecode().executable.clone();
    let instructions = original.template.clone();
    // Scalar TRUE selects [1,2,3]; the array predicate selects [1,10,3].
    // B1 is broadcast over all three rows, with ordinary numeric text coercion.
    for (condition, operand, expected) in [
        (true, CellValue::Number(2.), 12.),
        (false, CellValue::Number(2.), 20.),
        (false, CellValue::Text("3".into()), 23.),
        (true, CellValue::Number(2.), 12.),
    ] {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet.set_value(target("A1").address, condition).unwrap();
        sheet.set_value(target("B1").address, operand).unwrap();
        let plan = compiler.compile_workbook(&book, &policy);
        let config =
            RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::DynamicSpill);
        let mut run = EvaluationRun::new_with_configuration(
            WorkbookView::new(&book, &plan, &policy).unwrap(),
            &config,
            Default::default(),
        )
        .unwrap();
        assert_eq!(
            run.evaluate_formula(target("D1"), &prepared).unwrap(),
            CellValue::Number(expected)
        );
        assert!(std::sync::Arc::ptr_eq(
            &original,
            &formula.bytecode().executable
        ));
        assert_eq!(formula.bytecode().template, instructions);
    }
}

#[test]
fn stack_broadcasts_both_axes_and_preserves_selected_errors() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&book, &policy);
    let config =
        RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::DynamicSpill);
    let mut run = EvaluationRun::new_with_configuration(
        WorkbookView::new(&book, &plan, &policy).unwrap(),
        &config,
        Default::default(),
    )
    .unwrap();
    for (source, expected) in [
        ("SUM({1,2}+{10;20})", CellValue::Number(66.)),
        ("SUM({10;20}+{1,2})", CellValue::Number(66.)),
        ("SUM({1,2}+{10,20,30})", CellValue::Error(CellError::Value)),
        (
            "SUM(IF({TRUE;FALSE;TRUE},{1;2;3},{100;200;300}))",
            CellValue::Number(204.),
        ),
        (
            "SUM(IF({TRUE;TRUE;TRUE},{1;2;3},1/0))",
            CellValue::Number(6.),
        ),
        (
            "SUM(IF({TRUE;FALSE;TRUE},{1;2;3},1/0))",
            CellValue::Error(CellError::Div0),
        ),
    ] {
        let prepared = compiler.compile_formula(source, &policy);
        assert_eq!(
            run.evaluate_formula(target("D1"), &prepared).unwrap(),
            expected,
            "{source}"
        );
    }
}
