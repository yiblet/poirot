use poirot::eval::{EvaluationBudgets, EvaluationOutcome, run_formula};
use poirot::model::{CellError, CellValue, Workbook};

fn workbook() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    sheet.set_value("A1", 3.).unwrap();
    sheet.set_value("A2", 7.).unwrap();
    sheet.set_formula("A3", "LET(inner,Rate,inner)").unwrap();
    book.define_name("Rate", "10", None).unwrap();
    book
}

fn value(book: &Workbook, formula: &str) -> CellValue {
    match run_formula(
        book,
        0,
        "B1".parse().unwrap(),
        formula,
        Default::default(),
        Default::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("{formula}: {outcome:?}"),
    }
}

#[test]
fn let_resolves_source_order_shadowing_and_deferred_reads() {
    let book = workbook();
    // Microsoft documents declaration order, formula-local names and one calculation per binding.
    // https://support.microsoft.com/en-us/excel/functions/let-function
    for (formula, expected) in [
        ("LET(x,5,SUM(x,1))", 6.),
        ("LET(x,A1,y,x+1,y*x)", 12.),
        ("LET(x,5,LET(X,x+2,IF(TRUE,x*9,0))+x)", 68.),
        ("LET(Rate,Rate+1,A3+Rate)", 21.),
        ("_xlfn.LET(_xlpm.x,5,IF(TRUE,_xlpm.x,0))", 5.),
        ("LET(draw,RAND(),draw-draw)", 0.),
    ] {
        assert_eq!(value(&book, formula), CellValue::Number(expected));
    }
}

#[test]
fn let_keeps_references_arrays_and_worksheet_errors_in_their_value_channels() {
    let book = workbook();
    for (formula, expected) in [
        ("LET(data,A1:A2,SUM(data))", CellValue::Number(10.)),
        ("SUM(LET(data,A1:A2,data))", CellValue::Number(10.)),
        ("LET(data,SEQUENCE(2),SUM(data))", CellValue::Number(3.)),
        (
            "LET(problem,1/0,problem)",
            CellValue::Error(CellError::Div0),
        ),
        ("LET(problem,1/0,IFERROR(problem,7))", CellValue::Number(7.)),
        // Reported in Excel 2024 for unused and transitively used bindings.
        // https://www.helpaso.net/course/e_function/logical/let/
        ("LET(problem,1/0,7)", CellValue::Number(7.)),
        (
            "LET(problem,1/0,derived,problem+7,derived)",
            CellValue::Error(CellError::Div0),
        ),
        ("LET(c,2,c)", CellValue::Error(CellError::Value)),
        ("LET(A1,2,3)", CellValue::Error(CellError::Value)),
        ("LET(x,2)", CellValue::Error(CellError::Value)),
    ] {
        assert_eq!(value(&book, formula), expected, "{formula}");
    }
}

#[test]
fn let_initializers_are_eager_even_when_the_result_never_reads_them() {
    let book = workbook();
    let formula = "LET(unused,SEQUENCE(1000),7)";
    let constrained = EvaluationBudgets {
        run_work: 96,
        ..EvaluationBudgets::default()
    };
    assert!(matches!(
        run_formula(
            &book,
            0,
            "B1".parse().unwrap(),
            formula,
            Default::default(),
            constrained,
        ),
        EvaluationOutcome::Aborted { .. }
    ));
    assert_eq!(value(&book, formula), CellValue::Number(7.));
}

#[test]
fn let_preparation_marks_lexical_callable_reference_selection() {
    use poirot::eval::compile::compiler::{FormulaCompiler, PreparationPolicy, PreparedFormula};

    let mut compiler = FormulaCompiler::new();
    let policy = PreparationPolicy::default();
    for (source, dynamic) in [
        ("LET(reducer,NamedCallable,reducer(A1))", true),
        ("LET(value,A1,value+1)", false),
    ] {
        let prepared = compiler.compile_formula(source, &policy);
        let PreparedFormula::Ready(compiled) = prepared.as_ref() else {
            panic!("failed to prepare {source}");
        };
        assert_eq!(
            compiled.potential_dependencies().dynamic_references,
            dynamic
        );
    }
}

#[test]
fn let_prepared_slots_survive_snapshots_and_track_cell_and_workbook_name_reads() {
    use poirot::calculation::EvaluationContext;
    use poirot::snapshots::PreparedPolicy;

    let mut book = workbook();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("B1", "LET(local,A1,Rate,SUM(local,A2),IF(TRUE,Rate+A3,0))")
        .unwrap();
    let context = EvaluationContext::default();
    let mut scenario = book.overlay().build().unwrap();
    let input = scenario.target("Data!A1").unwrap();
    let output = scenario.target("Data!B1").unwrap();
    assert_eq!(
        scenario.evaluation(&context).unwrap().cell(output).unwrap(),
        CellValue::Number(20.)
    );
    let bytes = scenario.save_snapshot().to_buffer().unwrap();
    let mut restored = Workbook::restore_snapshot()
        .prepared_policy(PreparedPolicy::RequireCompatible)
        .from_bytes(&bytes)
        .unwrap()
        .into_overlay()
        .unwrap();
    restored.set_value(input, 5.).unwrap();
    assert_eq!(
        restored.evaluation(&context).unwrap().cell(output).unwrap(),
        CellValue::Number(22.)
    );
    drop(scenario);
    book.named_ranges[0].formula = "20".into();
    assert_eq!(
        value(&book, "LET(local,A1,Rate,SUM(local,A2),IF(TRUE,Rate+A3,0))"),
        CellValue::Number(30.)
    );
}
