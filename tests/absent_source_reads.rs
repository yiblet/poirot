//! Reading an implicit blank must not author a cell during result publication.
use poirot::{
    CellStyle, CellValue, Evaluation, EvaluationContext, EvaluationTarget, Overlay, Spreadsheet,
    Workbook, WorkbookChange, io,
};

fn at(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}

fn book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, value) in [("A1", "a"), ("B1", "b"), ("B2", "d")] {
        sheet.set_value(at(cell).address, value).unwrap();
    }
    book
}

macro_rules! source_read_test {
    ($name:ident, $construct:expr, $read:expr) => {
        #[test]
        fn $name() {
            let source = book();
            let mut runtime = ($construct)(&source);
            let read = $read;
            // The exact issue029 reproducer, plus scalar, range, and selector consumers.
            for (formula, expected) in [
                ("CONCAT(,A2,\"\",\"x\")", CellValue::Text("x".into())),
                ("ISBLANK(A2)", CellValue::Boolean(true)),
                ("LEN(A2)", CellValue::Number(0.)),
                ("SUM(A2:B2)", CellValue::Number(0.)),
                ("ISBLANK(INDEX(A1:B2,2,1))", CellValue::Boolean(true)),
                ("ISBLANK(IF(TRUE,A2,7))", CellValue::Boolean(true)),
            ] {
                runtime.set_formula(at("Z100"), formula).unwrap();
                assert_eq!(read(&mut runtime, at("Z100")), expected, "{formula}");
                assert_eq!(read(&mut runtime, at("A2")), CellValue::Blank);
                let changes = runtime.changes();
                assert!(!changes.iter().any(|change| matches!(change,
                    WorkbookChange::SetValue { address, .. } if *address == at("A2").address
                )), "{formula}: {changes:?}");
                let mut applied = source.clone();
                applied.apply(changes).unwrap();
                assert!(applied.sheet(0).unwrap().get(at("A2").address).is_none());
            }
            // Authored blank/style edits must still create real cells and invalidate reads.
            runtime.set_value(at("A2"), 7.).unwrap();
            runtime.set_formula(at("Z100"), "ISBLANK(A2)").unwrap();
            assert_eq!(read(&mut runtime, at("Z100")), CellValue::Boolean(false));
            runtime.set_value(at("A2"), CellValue::Blank).unwrap();
            runtime.set_style(at("A2"), CellStyle { number_format: Some("0.00".into()) }).unwrap();
            assert_eq!(read(&mut runtime, at("Z100")), CellValue::Boolean(true));
            let changes = runtime.changes();
            assert!(changes.iter().any(|change| matches!(change,
                WorkbookChange::SetValue { address, value: CellValue::Blank, .. }
                if *address == at("A2").address
            )));
            let mut applied = source.clone();
            applied.apply(changes).unwrap();
            let cell = applied.sheet(0).unwrap().get(at("A2").address).unwrap();
            assert_eq!(cell.value, CellValue::Blank);
            assert_eq!(cell.style.number_format.as_deref(), Some("0.00"));
            // Removing that override restores absence; the next read must not recreate it.
            runtime.remove_cell(at("A2")).unwrap();
            assert_eq!(read(&mut runtime, at("Z100")), CellValue::Boolean(true));
            let mut applied = source.clone();
            applied.apply(runtime.changes()).unwrap();
            assert!(applied.sheet(0).unwrap().get(at("A2").address).is_none());
            // Removing baseline content likewise preserves a real RemoveCell change.
            runtime.remove_cell(at("A1")).unwrap();
            assert_eq!(read(&mut runtime, at("A1")), CellValue::Blank);
            let changes = runtime.changes();
            assert!(changes.iter().any(|change| matches!(change,
                WorkbookChange::RemoveCell { address, .. } if *address == at("A1").address
            )));
            let mut applied = source.clone();
            applied.apply(changes).unwrap();
            assert!(applied.sheet(0).unwrap().get(at("A1").address).is_none());
            // Worksheet publication replaces a stale cache with zero and retains the formula.
            runtime.set_formula(at("Z100"), "IF(TRUE,,)").unwrap();
            runtime.set_cached_value(at("Z100"), 99.).unwrap();
            assert_eq!(read(&mut runtime, at("Z100")), CellValue::Number(0.));
            let changes = runtime.changes();
            assert!(changes.iter().any(|change| matches!(change,
                WorkbookChange::SetCachedValue { address, value: CellValue::Number(0.), .. }
                if *address == at("Z100").address
            )));
            let mut applied = source.clone();
            applied.apply(changes).unwrap();
            let bytes = Spreadsheet::from_workbook(applied)
                .plan_xlsx_save(io::WriteOptions::default()).unwrap().to_buffer().unwrap();
            let reopened = Spreadsheet::from_bytes(&bytes).unwrap();
            assert!(reopened.workbook().sheet(0).unwrap().get(at("A2").address).is_none());
            let cell = reopened.workbook().sheet(0).unwrap().get(at("Z100").address).unwrap();
            assert_eq!(cell.formula.as_deref(), Some("IF(TRUE,,)"));
            assert_eq!(cell.value, CellValue::Number(0.));
        }
    };
}

source_read_test!(
    direct_reads_preserve_source_presence,
    |source| Evaluation::with_workbook(source)
        .build(EvaluationContext::default())
        .unwrap(),
    |runtime: &mut Evaluation<'_>, target| runtime.cell(target).unwrap()
);
source_read_test!(
    overlay_reads_preserve_source_presence,
    |source| Overlay::with_workbook(source).build().unwrap(),
    |runtime: &mut Overlay<'_>, target| runtime
        .evaluation(&EvaluationContext::default())
        .unwrap()
        .cell(target)
        .unwrap()
);

#[test]
fn absent_reads_and_retained_results_survive_snapshot_without_authoring_cells() {
    let source = book();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&source)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&source).build().unwrap();
    let formula = "CONCAT(,A2,\"\",\"x\")";
    direct.set_formula(at("Z100"), formula).unwrap();
    overlay.set_formula(at("Z100"), formula).unwrap();
    let expected = CellValue::Text("x".into());
    assert_eq!(direct.cell(at("Z100")).unwrap(), expected);
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("Z100"))
            .unwrap(),
        expected
    );
    assert_eq!(direct.changes(), overlay.changes());
    let bytes = postcard::to_allocvec(&direct.snapshot()).unwrap();
    let (restored_direct, state) =
        Evaluation::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored_direct = state
        .into_evaluation(&restored_direct, context.clone())
        .unwrap();
    assert_eq!(restored_direct.cell(at("Z100")).unwrap(), expected);
    assert_eq!(direct.changes(), restored_direct.changes());
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (restored, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    assert!(
        state
            .source_workbook(&restored)
            .unwrap()
            .sheet(0)
            .unwrap()
            .get(at("A2").address)
            .is_none()
    );
    let mut overlay = state.attach(&restored).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("Z100")).unwrap(), expected);
    assert_eq!(evaluation.usage().formula_executions, 0);
    drop(evaluation);
    assert_eq!(direct.changes(), overlay.changes());
    overlay.set_value(at("A2"), "y").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("Z100"))
            .unwrap(),
        CellValue::Text("yx".into())
    );
}

#[test]
fn blank_spill_children_publish_zero_without_authoring_the_blank_input() {
    let source = book();
    let context = EvaluationContext::default();
    let policy = poirot::OverlayPolicy {
        builtin: poirot::eval::BuiltinSemanticsPolicy::ExcelDocumentation,
        arrays: poirot::eval::ArrayResultPolicy::DynamicSpill,
        ..Default::default()
    };
    let mut direct = Evaluation::with_workbook(&source)
        .with_policy(policy.clone())
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&source)
        .with_policy(policy)
        .build()
        .unwrap();
    direct.set_formula(at("Z100"), "TRANSPOSE(A1:A2)").unwrap();
    overlay.set_formula(at("Z100"), "TRANSPOSE(A1:A2)").unwrap();
    assert_eq!(
        direct.cell(at("Z100")).unwrap(),
        CellValue::Text("a".into())
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("Z100"))
            .unwrap(),
        CellValue::Text("a".into())
    );
    assert_eq!(direct.cell(at("AA100")).unwrap(), CellValue::Number(0.));
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("AA100"))
            .unwrap(),
        CellValue::Number(0.)
    );
    let changes = direct.changes();
    assert_eq!(changes, overlay.changes());
    assert!(changes.iter().any(|change| matches!(change,
        WorkbookChange::SetSpillCachedValue { address, value: CellValue::Number(0.), .. }
        if *address == at("AA100").address
    )));
    let mut applied = source.clone();
    applied.apply(changes).unwrap();
    assert!(applied.sheet(0).unwrap().get(at("A2").address).is_none());
}
