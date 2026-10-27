use crate::eval::compile;
use crate::eval::config::{EvaluationInputs, RunConfiguration};
use crate::eval::entry::run_formula_with_configuration;
use crate::eval::limits::{EvaluationBudgets, EvaluationError, EvaluationOutcome};
use crate::eval::ports::{DependencyRead, DependencyReadSink, ReadEvent};
use crate::eval::session::{ResultProjection, Session};
use crate::eval::vm::ExpressionRunner;
use crate::model::CellError::{NA, Ref};
use crate::model::CellValue::{Error, Number, Text};
use crate::model::{CellValue, Workbook};

fn book() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    w
}

fn value(w: &Workbook, formula: &str) -> CellValue {
    match run_formula_with_configuration(
        w,
        0,
        "Z20".parse().unwrap(),
        formula,
        &RunConfiguration::default(),
        EvaluationBudgets::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{formula}: {other:?}"),
    }
}

#[test]
fn approximate_match_preserves_verified_typed_candidate_rules() {
    let w = book();
    let excel = true;
    // Pinned ClosedXML LookupTests (09f2f21) independently supplies these typed
    // expectations, including deliberate unordered trajectories. Calc agrees on
    // the originals but differs on descending duplicates and mixed/error entries.
    for (key, vector, mode, expected) in [
        ("5", "TRUE", -1, Error(NA)),
        ("12", "5,15,18,18,11,1,15,17", 1, Number(1.)),
        ("17", "14,5,3,5,11,12,11,13,13,4", 1, Number(10.)),
        ("5", "10,4,5", -1, Number(1.)),
        ("5", "10,5,4,5,5,5,5,5", -1, Number(2.)),
        ("5", "\"5\",10,\"4\",FALSE,TRUE,#DIV/0!,5,3", -1, Number(7.)),
        ("6", "\"4\",10,\"4\",FALSE,TRUE,#DIV/0!,5,3", -1, Number(2.)),
        ("5", "\"5\"", -1, Error(NA)),
        ("\"c\"", "\"E\",4,\"D\",\"B\"", -1, Number(3.)),
        ("FALSE", "TRUE,TRUE,\"FALSE\",0,FALSE,FALSE", -1, Number(5.)),
        ("39", "25,38,38,38,40,41", 1, Number(4.)),
        ("20", "25,38,40", 1, Error(NA)),
        ("25", "20,TRUE,FALSE,38,40", 1, Number(1.)),
        ("8", "FALSE,FALSE", 1, Error(NA)),
        ("5", "1,2,3", 1, Number(3.)),
        ("4", "10,3,FALSE,FALSE,FALSE", 1, Error(NA)),
        ("8", "14,0,17,FALSE,8", 1, Error(NA)),
        ("35", "\"35\",38,24,35,70", 0, Number(4.)),
        ("TRUE", "0,\"TRUE\",FALSE,TRUE,1", 0, Number(4.)),
    ] {
        for separator in [",", ";"] {
            let formula = format!("MATCH({key},{{{}}},{mode})", vector.replace(',', separator));
            assert_eq!(value(&w, &formula,), expected, "{formula}");
        }
    }
    {}
    for _policy in [excel] {
        for (formula, expected) in [
            (
                "VLOOKUP(12,{5,1;15,2;18,3;18,4;11,5;1,6;15,7;17,8},2,TRUE)",
                Number(1.),
            ),
            (
                "HLOOKUP(12,{5,15,18,18,11,1,15,17;1,2,3,4,5,6,7,8},2,TRUE)",
                Number(1.),
            ),
            ("MATCH(5,{10,#REF!,5,3},0)", Number(3.)),
        ] {
            assert_eq!(value(&w, formula,), expected, "{formula}");
        }
    }
}

#[test]
fn approximate_match_preserves_blank_positions_lazy_failures_and_policy_dependencies() {
    #[derive(Default)]
    /// Records MATCH dependencies so tests can verify lazy failures, blank positions, and consumed policies.
    struct Reads(Vec<ReadEvent>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event);
        }
    }
    let mut w = book();
    let sheet = w.sheet_mut(0).unwrap();
    sheet.set_value("A1", 10.).unwrap();
    sheet.set_value("A3", Text("5".into())).unwrap();
    sheet.set_value("A4", Error(Ref)).unwrap();
    sheet.set_value("A5", 5.).unwrap();
    sheet.set_formula("A6", "RAND()").unwrap();
    sheet.set_value("B1", 5.).unwrap();
    sheet.set_value("C1", -1.).unwrap();
    {
        let mut reads = Reads::default();
        let mut session = Session::with_context(&w, EvaluationInputs::default());
        session.ports.dependency_sink = Some(&mut reads);
        let syntax = poirot_parsing::formula::parse_formula("MATCH(B1,A1:A6,C1)").unwrap();
        let plan = compile::bound::FormulaPlan::bind(&syntax).unwrap();
        let at = "Z20".parse().unwrap();
        let result = ExpressionRunner::new(&mut session)
            .expr(0, at, &plan.expression())
            .unwrap();
        assert_eq!(
            ResultProjection::new(&mut session)
                .select_legacy_scalar(at, result)
                .unwrap(),
            Number(5.)
        );
        drop(session);
        let cells: Vec<_> = reads
            .0
            .iter()
            .filter_map(|e| match &e.read {
                DependencyRead::Cell(t) => Some(t.address),
                _ => None,
            })
            .collect();
        assert_eq!(
            cells,
            ["C1", "B1", "A1", "A3", "A4", "A5"].map(|s| s.parse().unwrap())
        );
        assert!(
            !reads
                .0
                .iter()
                .any(|e| matches!(e.read, DependencyRead::Random(_)))
        );
    }

    assert_eq!(value(&w, "MATCH(0,A1:A5,0)",), Error(NA));
    assert_eq!(value(&w, "MATCH(6,A1:A5,-1)",), Number(1.));
    assert_eq!(value(&w, "MATCH(5,A2:A5,-1)",), Number(4.));
    w.sheet_mut(0)
        .unwrap()
        .set_value("A4", Text("oversized".into()))
        .unwrap();
    let configuration = RunConfiguration::default();
    for budgets in [
        EvaluationBudgets {
            text_bytes: 1,
            ..Default::default()
        },
        EvaluationBudgets {
            run_work: 1,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            run_formula_with_configuration(
                &w,
                0,
                "Z20".parse().unwrap(),
                "IFERROR(MATCH(5,A1:A5,-1),99)",
                &configuration,
                budgets
            ),
            EvaluationOutcome::Aborted {
                failure: EvaluationError::Budget(_),
                ..
            }
        ));
    }
    // The error in the mode is resolved before either the key or candidate providers.
    assert_eq!(
        value(&w, "IFERROR(MATCH(RAND(),A1:A5,#DIV/0!),99)",),
        Number(99.)
    );
}

#[test]
fn ascending_match_bisection_reads_only_consumed_candidates_in_original_order() {
    #[derive(Default)]
    /// Records candidate reads so ascending MATCH tests can verify bisection order and skipped cells.
    struct Reads(Vec<ReadEvent>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event);
        }
    }
    let mut w = book();
    for (index, number) in [5., 15., 18., 18., 11., 1., 15., 17.]
        .into_iter()
        .enumerate()
    {
        w.sheet_mut(0)
            .unwrap()
            .set_value(
                format!("A{}", index + 1)
                    .parse::<crate::model::CellAddress>()
                    .unwrap(),
                number,
            )
            .unwrap();
    }
    w.sheet_mut(0).unwrap().set_formula("A7", "RAND()").unwrap();
    let mut reads = Reads::default();
    let mut session = Session::with_context(&w, EvaluationInputs::default());

    session.ports.dependency_sink = Some(&mut reads);
    let syntax = poirot_parsing::formula::parse_formula("MATCH(12,A1:A8)").unwrap();
    let plan = compile::bound::FormulaPlan::bind(&syntax).unwrap();
    let at = "Z20".parse().unwrap();
    let result = ExpressionRunner::new(&mut session)
        .expr(0, at, &plan.expression())
        .unwrap();
    assert_eq!(
        ResultProjection::new(&mut session)
            .select_legacy_scalar(at, result)
            .unwrap(),
        Number(1.)
    );
    drop(session);
    let cells: Vec<_> = reads
        .0
        .iter()
        .filter_map(|e| match &e.read {
            DependencyRead::Cell(t) => Some(t.address),
            _ => None,
        })
        .collect();
    assert_eq!(cells, ["A4", "A2", "A1"].map(|s| s.parse().unwrap()));
    assert!(
        !reads
            .0
            .iter()
            .any(|e| matches!(e.read, DependencyRead::Random(_)))
    );
    // A sparse blank retains its logical position in a documented sorted vector.
    let mut w = book();
    for (cell, number) in [("A1", 1.), ("A3", 3.), ("A4", 5.)] {
        w.sheet_mut(0).unwrap().set_value(cell, number).unwrap();
    }
    assert_eq!(value(&w, "MATCH(4,A1:A4,1)",), Number(3.));
}
