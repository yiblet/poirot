use poirot_eval::*;
use poirot_model::{CellValue, Workbook};
fn target(at: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: at.parse().unwrap(),
    }
}
fn config() -> RunConfiguration {
    RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::DynamicSpill)
}
fn workbook() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("S").unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value(target("D1").address, 17.)
        .unwrap();
    w
}
#[test]
fn exact_computation_identity_includes_array_publication_policy() {
    let scalar = RunConfiguration::default();
    let dynamic = config();
    assert_ne!(scalar.fingerprint(), dynamic.fingerprint());
    assert_ne!(
        scalar.fingerprint().computation(),
        dynamic.fingerprint().computation()
    );
    assert_eq!(
        scalar.array_result_policy(),
        ArrayResultPolicy::ScalarIntersection
    );
}
#[test]
fn array_probes_refuse_without_overwriting_source_and_scalar_probes_stay_isolated() {
    let w = workbook();
    let policy = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&w, &policy);
    let array = compiler.compile_formula("{1,2}", &policy);
    let bound = bound::FormulaPlan::bind(&poirot_formula::parse_formula("{3,4}").unwrap()).unwrap();
    let scalar = compiler.compile_formula("9", &policy);
    let configuration = config();
    let mut run = EvaluationRun::new_with_configuration(
        WorkbookView::new(&w, &plan, &policy).unwrap(),
        &configuration,
        Default::default(),
    )
    .unwrap();
    for result in [
        run.evaluate_formula(target("D1"), &array),
        run.evaluate_bound(target("D1"), &bound),
    ] {
        assert!(matches!(
            result,
            Err(EvaluationFailure::UnsupportedCapability { .. })
        ));
    }
    assert_eq!(
        run.evaluate_formula(target("D1"), &scalar).unwrap(),
        CellValue::Number(9.)
    );
    assert_eq!(
        run.evaluate_cell(target("D1")).unwrap(),
        CellValue::Number(17.)
    );
    assert_eq!(run.evaluate_cell(target("E1")).unwrap(), CellValue::Blank);
    let retained = run.finish_retaining_cells(Ok(()));
    assert_eq!(retained.memo_usage.cells, 2);
}
#[test]
fn default_array_probes_keep_scalar_intersection() {
    let w = workbook();
    let policy = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&w, &policy);
    let array = compiler.compile_formula("{1,2}", &policy);
    let mut run = EvaluationRun::new(
        WorkbookView::new(&w, &plan, &policy).unwrap(),
        Default::default(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(
        run.evaluate_formula(target("D1"), &array).unwrap(),
        CellValue::Number(1.)
    );
    assert_eq!(
        run.evaluate_cell(target("D1")).unwrap(),
        CellValue::Number(17.)
    );
}

#[test]
fn child_discovery_records_anchor_and_selected_source_dependencies() {
    #[derive(Default)]
    /// Records producer and source dependencies discovered through a spill child.
    struct Sink(Vec<ReadEvent>);
    impl DependencyReadSink for Sink {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event);
        }
    }
    let mut w = Workbook::new();
    w.add_sheet("S").unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_formula(
            target("D1").address,
            "IF(TRUE,TRANSPOSE(A1:A2),TRANSPOSE(B1:B2))",
            CellValue::Blank,
        )
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value(target("A1").address, 1.)
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value(target("A2").address, 2.)
        .unwrap();
    let policy = EvaluationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&w, &policy);
    let configuration = RunConfiguration::default()
        .with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation)
        .with_array_result_policy(ArrayResultPolicy::DynamicSpill);
    let mut sink = Sink::default();
    let mut run = EvaluationRun::new_with_configuration(
        WorkbookView::new(&w, &plan, &policy).unwrap(),
        &configuration,
        Default::default(),
    )
    .unwrap()
    .with_dependency_sink(&mut sink);
    assert_eq!(
        run.evaluate_cell(target("E1")).unwrap(),
        CellValue::Number(2.)
    );
    run.finish(Ok(()));
    assert!(
        sink.0
            .iter()
            .any(|event| event.read == DependencyRead::Cell(target("D1")))
    );
    assert!(
        sink.0
            .iter()
            .any(|event| event.consumer == Some(target("D1"))
                && event.read == DependencyRead::Cell(target("A2")))
    );
    assert!(
        !sink
            .0
            .iter()
            .any(|event| event.read == DependencyRead::Cell(target("B1"))
                || event.read == DependencyRead::Cell(target("B2")))
    );
}

#[test]
fn mutable_recalculation_refuses_dynamic_publication_before_mutation() {
    let mut w = workbook();
    w.sheet_mut(0)
        .unwrap()
        .set_formula(target("F1").address, "{1,2}", CellValue::Number(99.))
        .unwrap();
    let original = w.clone();
    let report = run_recalculation_with_configuration(&mut w, &config(), Default::default());
    assert!(matches!(
        report,
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::UnsupportedCapability { .. },
            ..
        }
    ));
    assert_eq!(w, original);
}

#[test]
fn scalar_external_cache_refuses_dynamic_spills_before_reader_invocation() {
    use std::sync::Arc;
    #[derive(Default)]
    /// Counts cache callbacks to verify unsupported dynamic reuse is rejected first.
    struct Reader {
        calls: usize,
    }
    impl CachedCellReader for Reader {
        fn cached_cell(
            &mut self,
            _: EvaluationTarget,
        ) -> Result<Option<Arc<CellValue>>, EvaluationFailure> {
            self.calls += 1;
            Ok(Some(Arc::new(CellValue::Number(1.))))
        }
    }
    let mut w = workbook();
    w.sheet_mut(0)
        .unwrap()
        .set_formula(target("D1").address, "{1,2}", CellValue::Blank)
        .unwrap();
    let policy = EvaluationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&w, &policy);
    for first in ["D1", "E1"] {
        let configuration = config();
        let mut reader = Reader::default();
        let mut run = EvaluationRun::new_with_configuration(
            WorkbookView::new(&w, &plan, &policy).unwrap(),
            &configuration,
            Default::default(),
        )
        .unwrap()
        .with_cell_reader(&mut reader);
        let result = run.evaluate_cell(target(first));
        assert!(matches!(
            result,
            Err(EvaluationFailure::UnsupportedCapability { .. })
        ));
        let retained = run.finish_retaining_cells(result);
        assert!(retained.cells.is_empty());
        assert_eq!(retained.memo_usage.cells, 0);
        assert_eq!(reader.calls, 0);
    }
    let mut reader = Reader::default();
    let mut run = EvaluationRun::new(
        WorkbookView::new(&w, &plan, &policy).unwrap(),
        Default::default(),
        Default::default(),
    )
    .unwrap()
    .with_cell_reader(&mut reader);
    assert_eq!(
        run.evaluate_cell(target("D1")).unwrap(),
        CellValue::Number(1.)
    );
    run.finish(Ok(()));
    assert_eq!(reader.calls, 1);
}
