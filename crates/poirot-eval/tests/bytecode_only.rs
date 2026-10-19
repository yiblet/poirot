//! Exercise production-only execution, without the unit-test tree oracle.
use poirot_eval::{
    EvaluationBudgets, EvaluationFailure, EvaluationInputs, EvaluationPolicy, EvaluationRun,
    EvaluationTarget, FormulaCompiler, PreparedFormula, WorkbookView, bytecode,
    overlay::{EvaluationContext as JobContext, Overlay, snapshot::Snapshot},
};
use poirot_model::{CellError, CellValue, Workbook};
use std::sync::Arc;

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

#[test]
fn restored_programs_preserve_reference_channels_lazy_errors_and_omissions() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value(at("A1").address, 2.).unwrap();
    sheet.set_value(at("A2").address, 3.).unwrap();
    sheet.set_value(at("B1").address, 7.).unwrap();
    let cases = [
        ("D1", "SUM((A1:A2,B1))", 12.),
        ("D2", "SUM(A1:B1 B1:B2)", 7.),
        ("D3", "IF(FALSE,Broken,SUM(1,,2))", 3.),
        ("D4", "IF(TRUE,INDEX({4,5},1,2),UNKNOWN())", 5.),
        ("D5", "SUM(INDIRECT(\"A1:A2\"))", 5.),
    ];
    for (address, source, _) in cases {
        sheet
            .set_formula(at(address).address, source, CellValue::Blank)
            .unwrap();
    }
    book.define_name("Broken", "1+", None).unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    overlay.compile_all();
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let snapshot: Snapshot<'static> = postcard::from_bytes(&bytes).unwrap();
    let (restored_book, state) = Overlay::<&Workbook>::from_snapshot(snapshot).unwrap();
    let mut restored = state.attach(&restored_book).unwrap();
    let context = JobContext::default();
    let mut evaluation = restored.evaluation(&context).unwrap();
    for (address, _, expected) in cases {
        assert_eq!(
            evaluation.cell(at(address)).unwrap(),
            CellValue::Number(expected),
            "{address}"
        );
    }
    drop(evaluation);
    restored.set_formula(at("D3"), "IF(TRUE,Broken,3)").unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("D3"))
            .unwrap(),
        CellValue::Error(CellError::Value)
    );
}

#[test]
fn explicit_lowering_failure_is_an_operational_abort_when_reached() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    let policy = EvaluationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
    let view = WorkbookView::new(&book, &plan, &policy).unwrap();
    let mut run = EvaluationRun::new(
        view,
        EvaluationInputs::default(),
        EvaluationBudgets::default(),
    )
    .unwrap();
    let failure = PreparedFormula::CompilationFailure {
        source: Arc::from("unlowerable"),
        policy,
        error: bytecode::CompileError::ResourceLimit,
    };
    assert!(matches!(
        run.evaluate_formula(at("A1"), &failure),
        Err(EvaluationFailure::ResourceLimit {
            capability: poirot_eval::budget::CapabilityId::FormulaCompilation,
            ..
        })
    ));
}
