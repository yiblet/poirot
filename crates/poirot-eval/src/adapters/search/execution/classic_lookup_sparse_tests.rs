use super::*;
#[derive(Default)]
/// Collects lookup point reads and consumed sparse membership guards.
struct Reads(Vec<ReadEvent>);
impl DependencyReadSink for Reads {
    fn read(&mut self, event: ReadEvent) {
        self.0.push(event);
    }
}
fn book() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    w
}
fn evaluate(w: &Workbook, formula: &str, budgets: EvaluationBudgets) -> CellValue {
    match run_formula(
        w,
        0,
        "Z20".parse().unwrap(),
        formula,
        EvaluationInputs::default(),
        budgets,
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{formula}: {other:?}"),
    }
}
#[test]
fn full_axes_misses_and_last_logical_positions_use_default_budgets() {
    let mut w = book();
    for (cell, value) in [("A1", 1.), ("B1", 10.), ("A2", 2.), ("B2", 20.)] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(cell.parse().unwrap(), value)
            .unwrap();
    }
    for formula in [
        "IFERROR(VLOOKUP(999,A:B,2,FALSE),7)",
        "IFERROR(HLOOKUP(999,1:2,2,FALSE),7)",
        "IFERROR(MATCH(999,A:A,0),7)",
    ] {
        assert_eq!(
            evaluate(&w, formula, EvaluationBudgets::default()),
            Number(7.)
        );
    }
    for (cell, value) in [("A1048576", 9.), ("B1048576", 42.)] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(cell.parse().unwrap(), value)
            .unwrap();
    }
    for (formula, expected) in [
        ("MATCH(9,A:A,0)", 1048576.),
        ("MATCH(9,A:A,1)", 1048576.),
        ("VLOOKUP(9,A:B,2,FALSE)", 42.),
        ("MATCH(0,A:A,-1)", 1048576.),
    ] {
        assert_eq!(
            evaluate(&w, formula, EvaluationBudgets::default()),
            Number(expected)
        );
    }
    w.sheet_mut(0)
        .unwrap()
        .set_value("XFD1".parse().unwrap(), 9.)
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value("XFD2".parse().unwrap(), 42.)
        .unwrap();
    assert_eq!(
        evaluate(&w, "HLOOKUP(9,1:2,2,FALSE)", EvaluationBudgets::default()),
        Number(42.)
    );
}
#[test]
fn first_match_does_not_reserve_or_read_populated_unused_suffix() {
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), 7.)
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value("B1".parse().unwrap(), 42.)
        .unwrap();
    for row in 1..100 {
        w.sheet_mut(0)
            .unwrap()
            .set_formula(CellAddress { row, col: 0 }, "UNSUPPORTED()", Number(7.))
            .unwrap();
    }
    let mut reads = Reads::default();
    let mut session = Session::new(
        &w,
        EvaluationInputs::default(),
        BudgetMeter::new(EvaluationBudgets {
            materialized_values: 2,
            ..EvaluationBudgets::default()
        })
        .unwrap(),
    );
    session.dependency_sink = Some(&mut reads);
    let syntax = poirot_parsing::formula::parse_formula("VLOOKUP(7,A:B,2,FALSE)").unwrap();
    let plan = bound::FormulaPlan::bind(&syntax).unwrap();
    let at = "Z20".parse().unwrap();
    let value = session.expr(0, at, &plan.expression()).unwrap();
    assert_eq!(session.scalar(at, value).unwrap(), Number(42.));
    assert_eq!(session.run_usage.formula_executions, 0);
    assert_eq!(session.cache.len(), 2);
    drop(session);
    assert!(
        !reads
            .0
            .iter()
            .any(|e| matches!(e.read, DependencyRead::SparseMembership { .. }))
    );
    let cells: Vec<_> = reads
        .0
        .into_iter()
        .filter_map(|e| match e.read {
            DependencyRead::Cell(t) => Some(t.address),
            _ => None,
        })
        .collect();
    assert_eq!(cells, ["A1", "B1"].map(|a| a.parse().unwrap()));
}
#[test]
fn absent_gap_has_membership_guard_and_stored_blank_formula_is_evaluated() {
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_formula("A10".parse().unwrap(), "IF(TRUE,C1,RAND())", Number(7.))
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value("A20".parse().unwrap(), 7.)
        .unwrap();
    let mut reads = Reads::default();
    let mut session = Session::with_context(&w, EvaluationInputs::default());
    session.dependency_sink = Some(&mut reads);
    let syntax = poirot_parsing::formula::parse_formula("MATCH(7,A:A,0)").unwrap();
    let plan = bound::FormulaPlan::bind(&syntax).unwrap();
    let at = "Z20".parse().unwrap();
    let result = session.expr(0, at, &plan.expression()).unwrap();
    assert_eq!(session.scalar(at, result).unwrap(), Number(20.));
    assert_eq!(session.run_usage.formula_executions, 1);
    assert_eq!(session.cache.len(), 3);
    drop(session);
    assert!(reads.0.iter().any(|e| matches!(e.read,DependencyRead::SparseMembership { first,last,.. } if first == "A1".parse().unwrap() && last == "A10".parse().unwrap())));
    assert!(
        !reads.0.iter().any(
            |e| matches!(e.read,DependencyRead::Cell(t) if t.address == "A1".parse().unwrap())
        )
    );
    assert!(
        !reads
            .0
            .iter()
            .any(|e| matches!(e.read, DependencyRead::Random(_)))
    );
}
#[test]
fn gaps_stop_duplicate_runs_and_preserve_descending_previous_logical_index() {
    let mut w = book();
    for (cell, value) in [
        ("A1", 1.),
        ("A4", 2.),
        ("A5", 2.),
        ("A6", 3.),
        ("B4", 20.),
        ("B5", 21.),
    ] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(cell.parse().unwrap(), value)
            .unwrap();
    }
    assert_eq!(
        evaluate(&w, "VLOOKUP(2,A1:B6,2,TRUE)", EvaluationBudgets::default()),
        Number(21.)
    );
    w.sheet_mut(0).unwrap().remove("A5".parse().unwrap());
    assert_eq!(
        evaluate(&w, "VLOOKUP(2,A1:B6,2,TRUE)", EvaluationBudgets::default()),
        Number(20.)
    );
    let mut descending = book();
    descending
        .sheet_mut(0)
        .unwrap()
        .set_value("A4".parse().unwrap(), 1.)
        .unwrap();
    assert_eq!(
        evaluate(
            &descending,
            "MATCH(2,A1:A6,-1)",
            EvaluationBudgets::default()
        ),
        Number(3.)
    );
}
#[test]
fn stored_formula_scalar_projection_remains_operational_and_lazy_branch_has_no_query() {
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_formula("A10".parse().unwrap(), "LEFT(UNICHAR(128512),1)", Blank)
        .unwrap();
    assert_eq!(
        evaluate(&w, "MATCH(\"*\",A:A,0)", EvaluationBudgets::default()),
        Number(10.)
    );
    let poi = RunConfiguration::default().with_text_policy(TextSemanticsPolicy {
        source: TextSourceVersion::Poi7321209OpenJdk21Unicode15,
    });
    assert!(matches!(
        run_formula_with_configuration(
            &w,
            0,
            "Z20".parse().unwrap(),
            "IFERROR(MATCH(\"*\",A:A,0),99)",
            &poi,
            EvaluationBudgets::default(),
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::UnsupportedCapability {
                capability: CapabilityId::FunctionAdapter,
                ..
            },
            ..
        }
    ));
    let mut reads = Reads::default();
    let mut session = Session::with_context(&w, EvaluationInputs::default());
    session.dependency_sink = Some(&mut reads);
    let syntax = poirot_parsing::formula::parse_formula("IF(FALSE,MATCH(7,A:A,0),42)").unwrap();
    let plan = bound::FormulaPlan::bind(&syntax).unwrap();
    let at = "Z20".parse().unwrap();
    let result = session.expr(0, at, &plan.expression()).unwrap();
    assert_eq!(session.scalar(at, result).unwrap(), Number(42.));
    drop(session);
    assert!(!reads.0.iter().any(|e| matches!(
        e.read,
        DependencyRead::SparseMembership { .. } | DependencyRead::Cell(_)
    )));
}
#[test]
fn sparse_selection_preserves_dense_results_and_stored_callback_order() {
    /// Sparse candidate fixture recording logical positions inspected by selection.
    struct Sparse<'a> {
        values: &'a [Option<Option<Ordering>>],
        reads: Vec<usize>,
    }
    impl selection::CandidateReader for Sparse<'_> {
        fn next(&mut self, first: usize, end: usize) -> FunctionResult<Option<usize>> {
            Ok((first..end).find(|i| self.values[*i].is_some()))
        }
        fn previous(&mut self, end: usize) -> FunctionResult<Option<usize>> {
            Ok((0..end).rev().find(|i| self.values[*i].is_some()))
        }
        fn compare(&mut self, index: usize) -> FunctionResult<Option<Ordering>> {
            self.reads.push(index);
            Ok(self.values[index].unwrap())
        }
    }
    let alphabet = [
        None,
        Some(None),
        Some(Some(Ordering::Less)),
        Some(Some(Ordering::Equal)),
        Some(Some(Ordering::Greater)),
    ];
    for len in 0..=5u32 {
        for encoded in 0..5usize.pow(len) {
            let mut digits = encoded;
            let values: Vec<_> = (0..len)
                .map(|_| {
                    let value = alphabet[digits % 5];
                    digits /= 5;
                    value
                })
                .collect();
            for (mode, lookup) in [
                (SearchMode::Exact, false),
                (SearchMode::Ascending, false),
                (SearchMode::Descending, false),
                (SearchMode::Exact, true),
                (SearchMode::Ascending, true),
            ] {
                let mut dense_reads = vec![];
                let dense = historical_selection::select_index(values.len(), mode, lookup, |i| {
                    if values[i].is_some() {
                        dense_reads.push(i);
                    }
                    Ok(values[i].flatten())
                })
                .unwrap();
                let mut sparse = Sparse {
                    values: &values,
                    reads: vec![],
                };
                let selected = select_with_reader(values.len(), mode, lookup, &mut sparse).unwrap();
                assert_eq!(selected, dense, "{values:?} {mode:?} lookup={lookup}");
                assert_eq!(
                    sparse.reads, dense_reads,
                    "{values:?} {mode:?} lookup={lookup}"
                );
            }
        }
    }
}
#[test]
fn stored_random_and_error_formulas_keep_actual_effects_despite_stale_caches() {
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_formula("A10".parse().unwrap(), "RAND()", Number(9.))
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_formula("A20".parse().unwrap(), "1/0", Number(9.))
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value("A30".parse().unwrap(), 9.)
        .unwrap();
    let mut reads = Reads::default();
    let mut session = Session::with_context(&w, EvaluationInputs::default());
    session.dependency_sink = Some(&mut reads);
    let syntax = poirot_parsing::formula::parse_formula("MATCH(9,A:A,0)").unwrap();
    let plan = bound::FormulaPlan::bind(&syntax).unwrap();
    let at = "Z20".parse().unwrap();
    let result = session.expr(0, at, &plan.expression()).unwrap();
    assert_eq!(session.scalar(at, result).unwrap(), Number(30.));
    assert_eq!(session.run_usage.formula_executions, 2);
    assert_eq!(session.cache[&(0, "A20".parse().unwrap())], Error(Div0));
    drop(session);
    assert!(
        reads
            .0
            .iter()
            .any(|e| matches!(e.read, DependencyRead::Random(_)))
    );
}
