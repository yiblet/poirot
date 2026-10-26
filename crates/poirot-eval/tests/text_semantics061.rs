use poirot_eval::{
    EvaluationBudgets, EvaluationOutcome, RunConfiguration, TextSemanticsPolicy, TextSourceVersion,
    run_formula_with_configuration,
};
use poirot_model::{CellAddress, CellValue, Workbook};

fn run(formula: &str, configuration: &RunConfiguration) -> EvaluationOutcome<CellValue> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    run_formula_with_configuration(
        &workbook,
        0,
        CellAddress { row: 0, col: 0 },
        formula,
        configuration,
        EvaluationBudgets::default(),
    )
}

#[test]
fn public_configuration_selects_documented_pairs_or_utf16_len() {
    let legacy = RunConfiguration::default().with_text_policy(TextSemanticsPolicy {
        source: TextSourceVersion::ExcelCompatibilityV2Documented,
    });
    assert_eq!(
        legacy.text_policy().source,
        TextSourceVersion::ExcelCompatibilityV2Documented
    );
    let poi = legacy.clone().with_text_policy(TextSemanticsPolicy {
        source: TextSourceVersion::Poi7321209,
    });
    for (configuration, expected) in [(&legacy, 1.), (&poi, 2.)] {
        assert!(matches!(run("LEN(\"😀\")", configuration),
            EvaluationOutcome::Complete { value: CellValue::Number(value), .. } if value == expected));
    }
    assert_ne!(legacy.fingerprint(), poi.fingerprint());
}

#[test]
fn dependency_port_records_actual_text_policy_consumption() {
    use poirot_eval::{
        DependencyRead, DependencyReadSink, EvaluationPolicy, EvaluationRun, EvaluationTarget,
        FormulaCompiler, ReadEvent, WorkbookView,
    };
    #[derive(Default)]
    /// Records actual text-policy consumption for incremental invalidation assertions.
    struct Reads(Vec<ReadEvent>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event);
        }
    }
    let target = EvaluationTarget {
        sheet: 0,
        address: CellAddress { row: 0, col: 0 },
    };
    let configuration = RunConfiguration::default();
    for (formula, consumed) in [
        ("LEN(\"😀\")", true),
        ("\"x\"&\"y\"", true),
        ("IF(FALSE,LEN(\"😀\"),7)", false),
        ("TRIM(\" x \" )", false),
        ("#N/A&\"x\"", false),
    ] {
        let mut workbook = Workbook::new();
        workbook.add_sheet("Sheet1").unwrap();
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula(target.address, formula)
            .unwrap();
        let policy = EvaluationPolicy::default();
        let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
        let mut reads = Reads::default();
        {
            let mut run = EvaluationRun::new_with_configuration(
                WorkbookView::new(&workbook, &plan, &policy).unwrap(),
                &configuration,
                EvaluationBudgets::default(),
            )
            .unwrap()
            .with_dependency_sink(&mut reads);
            run.evaluate_cell(target).unwrap();
        }
        assert_eq!(
            reads
                .0
                .iter()
                .any(|event| event.read == DependencyRead::TextPolicy),
            consumed,
            "{formula}"
        );
    }
}
