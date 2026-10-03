use poirot_model::{CellAddress, CellError, CellValue, Workbook};

fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
}

#[test]
fn explicit_selection_preserves_scalar_types_and_uses_array_top_left() {
    let workbook = workbook();
    let at = CellAddress { row: 9, col: 25 };
    for (formula, expected) in [
        ("@42", CellValue::Number(42.)),
        ("@\"0042\"", CellValue::from("0042")),
        ("@TRUE", CellValue::Boolean(true)),
        ("@#N/A", CellValue::Error(CellError::NA)),
        ("@@\"text\"", CellValue::from("text")),
        ("@{\"text\",20;30,40}", CellValue::from("text")),
        ("@{#DIV/0!,20;30,40}", CellValue::Error(CellError::Div0)),
        ("@{10;20;30}", CellValue::Number(10.)),
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &workbook,
                0,
                at,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{formula}"
        );
    }
}

#[test]
fn explicit_selection_reads_vector_coordinate_and_rejects_ambiguous_or_outside_ranges() {
    let mut workbook = workbook();
    let sheet = workbook.sheet_mut(0).unwrap();
    for (row, col, value) in [
        (0, 0, 10.),
        (1, 0, 20.),
        (2, 0, 30.),
        (0, 1, 40.),
        (0, 2, 50.),
        (1, 1, 60.),
    ] {
        sheet.set_value(CellAddress { row, col }, value).unwrap();
    }
    // Invalid formulas elsewhere in a vector must not be read by @.
    sheet
        .set_formula(
            CellAddress { row: 2, col: 0 },
            "UNKNOWN_FUNCTION()",
            CellValue::Number(0.),
        )
        .unwrap();
    for (formula, at, expected) in [
        (
            "@A1:A3",
            CellAddress { row: 1, col: 25 },
            CellValue::Number(20.),
        ),
        (
            "@A1:C1",
            CellAddress { row: 9, col: 1 },
            CellValue::Number(40.),
        ),
        (
            "@A1:A3+2",
            CellAddress { row: 1, col: 25 },
            CellValue::Number(22.),
        ),
        (
            "@A1",
            CellAddress { row: 9, col: 25 },
            CellValue::Number(10.),
        ),
        (
            "@A1:A3",
            CellAddress { row: 9, col: 25 },
            CellValue::Error(CellError::Value),
        ),
        (
            "@A1:C1",
            CellAddress { row: 9, col: 25 },
            CellValue::Error(CellError::Value),
        ),
        (
            "@A1:B2",
            CellAddress { row: 1, col: 1 },
            CellValue::Error(CellError::Value),
        ),
        (
            "@(A1:A2,B1:B2)",
            CellAddress { row: 1, col: 25 },
            CellValue::Error(CellError::Value),
        ),
        (
            "@(A1:A2 A2:B2)",
            CellAddress { row: 9, col: 25 },
            CellValue::Number(20.),
        ),
        (
            "@(A1:A2 B1:B2)",
            CellAddress { row: 1, col: 25 },
            CellValue::Error(CellError::Null),
        ),
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &workbook,
                0,
                at,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{formula}"
        );
    }
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(CellAddress { row: 1, col: 0 }, 25.)
        .unwrap();
    assert_eq!(
        match poirot_eval::run_formula(
            &workbook,
            0,
            CellAddress { row: 1, col: 25 },
            "@A1:A3",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(25.)
    );
}

#[test]
fn explicit_selection_records_only_the_selected_cell_dependency() {
    use poirot_eval::{
        DependencyRead, DependencyReadSink, EvaluationBudgets, EvaluationInputs, EvaluationPolicy,
        EvaluationRun, EvaluationTarget, FormulaCompiler, ReadEvent, WorkbookView,
    };
    #[derive(Default)]
    /// Records dependencies to verify intersection reads only its selected cell.
    struct Reads(Vec<DependencyRead>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event.read);
        }
    }
    let mut workbook = workbook();
    let selected = EvaluationTarget {
        sheet: 0,
        address: CellAddress { row: 1, col: 0 },
    };
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(selected.address, "selected")
        .unwrap();
    let policy = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::default();
    let plan = compiler.compile_workbook(&workbook, &policy);
    let expression = compiler.compile_formula("@A1:A3", &policy);
    let mut reads = Reads::default();
    {
        let mut run = EvaluationRun::new(
            WorkbookView::new(&workbook, &plan, &policy).unwrap(),
            EvaluationInputs::default(),
            EvaluationBudgets::default(),
        )
        .unwrap()
        .with_dependency_sink(&mut reads);
        let outcome = run
            .evaluate_formula(
                EvaluationTarget {
                    sheet: 0,
                    address: CellAddress { row: 1, col: 25 },
                },
                &expression,
            )
            .unwrap();
        assert_eq!(outcome, CellValue::from("selected"));
    }
    let cells: Vec<_> = reads
        .0
        .into_iter()
        .filter_map(|read| match read {
            DependencyRead::Cell(target) => Some(target),
            _ => None,
        })
        .collect();
    assert_eq!(cells, vec![selected]);
}
