//! Independent contracts from Microsoft RANDARRAY/TRIMRANGE pages and interval arithmetic.
//! https://support.microsoft.com/en-us/excel/functions/randarray-function
//! https://support.microsoft.com/en-us/excel/functions/trimrange-function
//! Native Excel does not document the blank-text/reference edge policies tested separately below.
use poirot_eval::{
    overlay::{Evaluation, EvaluationContext, Overlay},
    *,
};
use poirot_model::{CellError, CellValue, Workbook};
fn at(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}
fn book() -> Workbook {
    let mut b = Workbook::new();
    b.add_sheet("S").unwrap();
    b
}
fn run(
    book: &Workbook,
    text: &str,
    seed: u64,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula(
        book,
        0,
        at("Z100").address,
        text,
        EvaluationInputs {
            randomness_seed: seed,
            ..Default::default()
        },
        budgets,
    )
}
fn value(out: EvaluationOutcome<CellValue>) -> CellValue {
    match out {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
fn number(out: EvaluationOutcome<CellValue>) -> f64 {
    match value(out) {
        CellValue::Number(n) => n,
        other => panic!("{other:?}"),
    }
}
#[test]
fn random_defaults_domains_and_mathematical_intervals() {
    let b = book();
    for call in ["RANDARRAY()", "RANDARRAY(,)", "RANDARRAY(1,1,0,1,FALSE)"] {
        let out = run(&b, call, 7, Default::default());
        assert_eq!(out.usage().random_accesses[0].draws, 1);
        assert_eq!(out.usage().random_accesses[0].words, 1);
        assert!((0.0..1.0).contains(&number(out)));
    }
    for call in [
        "RANDARRAY(0)",
        "RANDARRAY(1,0)",
        "RANDARRAY(1048577)",
        "RANDARRAY(1,16385)",
        "RANDARRAY(1,1,2,2)",
        "RANDARRAY(1,1,3,2)",
        "RANDARRAY(1,1,0.1,0.2,TRUE)",
        "RANDARRAY(1,1,0,1,FALSE,7)",
    ] {
        let out = run(&b, call, 7, Default::default());
        assert!(out.usage().random_accesses.is_empty(), "{call}");
        assert_eq!(value(out), CellValue::Error(CellError::Value), "{call}");
    }
    for seed in 0..32 {
        let n = number(run(
            &b,
            "INDEX(RANDARRAY(3,2,-5,11,TRUE),3,2)",
            seed,
            Default::default(),
        ));
        assert_eq!(n.fract(), 0.0);
        assert!((-5.0..=11.0).contains(&n));
        let n = number(run(
            &b,
            "RANDARRAY(1,1,-1e308,1e308)",
            seed,
            Default::default(),
        ));
        assert!(n.is_finite() && (-1e308..=1e308).contains(&n));
    }
}
#[test]
fn random_accounting_lazy_options_and_operational_limits() {
    let b = book();
    let out = run(&b, "SUM(RANDARRAY(3,4))", 17, Default::default());
    assert_eq!(
        (
            out.usage().random_accesses[0].draws,
            out.usage().random_accesses[0].words
        ),
        (12, 12)
    );
    let repeated = run(&b, "SUM(RANDARRAY(3,4))", 17, Default::default());
    assert_eq!(value(out), value(repeated));
    let out = run(&b, "IF(FALSE,RANDARRAY(3,4),9)", 17, Default::default());
    assert!(out.usage().random_accesses.is_empty());
    assert_eq!(value(out), 9.0.into());
    let out = run(&b, "SUM(RANDARRAY(1,2,RAND(),2))", 17, Default::default());
    assert_eq!(out.usage().random_accesses[0].draws, 3);
    let out = run(&b, "RANDARRAY(0,1,RAND(),2)", 17, Default::default());
    assert_eq!(out.usage().random_accesses[0].draws, 1);
    assert_eq!(value(out), CellValue::Error(CellError::Value));
    for (call, budget) in [
        (
            "IFERROR(SUM(RANDARRAY(100,100)),9)",
            EvaluationBudgets {
                materialized_values: 32,
                ..Default::default()
            },
        ),
        (
            "IFERROR(RANDARRAY(1,1,-1e30,1e30,TRUE),9)",
            EvaluationBudgets::default(),
        ),
    ] {
        let out = run(&b, call, 17, budget);
        assert!(out.usage().random_accesses.is_empty());
        assert!(matches!(out, EvaluationOutcome::Aborted { .. }));
    }
    let out = run(
        &b,
        "SUM(RANDARRAY(100,100,-9007199254740992,9007199254740992,TRUE))",
        19,
        Default::default(),
    );
    let trace = &out.usage().random_accesses[0];
    assert_eq!(trace.draws, 10_000);
    // The supported seeded stream exercises rejection in this wide integer interval.
    assert!(trace.words > trace.draws);
}
#[test]
fn random_spill_children_seed_cache_and_shape_edits() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("J1").address, 3.0).unwrap();
    s.set_formula(at("A1").address, "RANDARRAY(J1,2,-20,20,TRUE)")
        .unwrap();
    s.set_formula(at("E1").address, "RANDARRAY(2,2)").unwrap();
    let context = EvaluationContext {
        randomness_seed: 31,
        ..Default::default()
    };
    let mut direct = Evaluation::with_workbook(&b)
        .build(context.clone())
        .unwrap();
    let expected = direct.cell(at("B3")).unwrap();
    assert!(
        matches!(expected, CellValue::Number(n) if n.fract()==0.0 && (-20.0..=20.0).contains(&n))
    );
    assert_eq!(direct.cell(at("B3")).unwrap(), expected);
    direct.begin(context.clone()).unwrap();
    direct.cell(at("F2")).unwrap();
    assert_eq!(direct.cell(at("B3")).unwrap(), expected);
    let next = EvaluationContext {
        randomness_seed: 32,
        ..context.clone()
    };
    direct.begin(next.clone()).unwrap();
    let mut fresh = Evaluation::with_workbook(&b).build(next).unwrap();
    assert_eq!(
        direct.cell(at("B3")).unwrap(),
        fresh.cell(at("B3")).unwrap()
    );
    direct.set_value(at("J1"), 1.0).unwrap();
    assert_eq!(direct.cell(at("B3")).unwrap(), CellValue::Blank);
    let mut overlay = Overlay::with_workbook(b).build().unwrap();
    {
        let mut e = overlay.evaluation(&context).unwrap();
        assert_eq!(e.cell(at("B3")).unwrap(), expected);
        e.cell(at("A1")).unwrap();
        assert_eq!(e.usage().formula_executions, 1);
    }
    {
        let mut e = overlay.evaluation(&context).unwrap();
        assert_eq!(e.cell(at("B3")).unwrap(), expected);
        assert_eq!(e.usage().formula_executions, 0);
    }
    let changed = EvaluationContext {
        randomness_seed: 32,
        ..context.clone()
    };
    {
        let mut e = overlay.evaluation(&changed).unwrap();
        e.cell(at("B3")).unwrap();
        assert_eq!(e.usage().formula_executions, 1);
    }
    overlay.set_value(at("J1"), 1.0).unwrap();
    assert_eq!(
        overlay
            .evaluation(&changed)
            .unwrap()
            .cell(at("B3"))
            .unwrap(),
        CellValue::Blank
    );
    overlay.set_value(at("B1"), 4.0).unwrap();
    assert_eq!(
        overlay
            .evaluation(&changed)
            .unwrap()
            .cell(at("A1"))
            .unwrap(),
        CellValue::Error(CellError::Spill)
    );
}
fn trim_book() -> Workbook {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    for (cell, n) in [("C3", 0.0), ("D4", 8.0), ("E5", 6.0)] {
        s.set_value(at(cell).address, n).unwrap();
    }
    b
}
#[test]
fn trim_all_axis_modes_and_interior_gaps() {
    let b = trim_book();
    for rows in 0..4 {
        for columns in 0..4 {
            let call = format!("TRIMRANGE(A1:G7,{rows},{columns})");
            let height = 7 - usize::from(rows & 1 != 0) * 2 - usize::from(rows & 2 != 0) * 2;
            let width = 7 - usize::from(columns & 1 != 0) * 2 - usize::from(columns & 2 != 0) * 2;
            assert_eq!(
                number(run(&b, &format!("ROWS({call})"), 0, Default::default())),
                height as f64
            );
            assert_eq!(
                number(run(&b, &format!("COLUMNS({call})"), 0, Default::default())),
                width as f64
            );
        }
    }
    assert_eq!(
        number(run(&b, "SUM(TRIMRANGE(A1:G7))", 0, Default::default())),
        14.0
    );
    assert_eq!(
        number(run(
            &b,
            "INDEX(TRIMRANGE(A1:G7),2,2)",
            0,
            Default::default()
        )),
        8.0
    );
    for call in [
        "TRIMRANGE(A1:G7,4)",
        "TRIMRANGE(A1:G7,-1)",
        "TRIMRANGE()",
        "TRIMRANGE(A1:G7,0,0,0)",
    ] {
        assert_eq!(
            value(run(&b, call, 0, Default::default())),
            CellValue::Error(CellError::Value)
        );
    }
}
#[test]
fn trim_original_blanks_reference_origin_and_explicit_edge_policy() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("B2").address, true).unwrap();
    s.set_value(at("C3").address, "").unwrap();
    s.set_value(at("D4").address, CellValue::Error(CellError::NA))
        .unwrap();
    // Policy under native verification: empty text is occupied; empty trimmed results are #CALC!;
    // references remain references, while scalar/array arguments produce arrays.
    for (call, expected) in [
        ("ROWS(TRIMRANGE(A1:E5))", 3.0.into()),
        ("ISREF(TRIMRANGE(A1:E5))", true.into()),
        ("ISREF(TRIMRANGE({1,2;3,4}))", false.into()),
        ("ROW(INDEX(TRIMRANGE(A1:E5),1,1))", 2.0.into()),
        ("COUNT(TRIMRANGE(A1:C3))", 0.0.into()),
        ("SUM(TRIMRANGE(A1:C3))", 0.0.into()),
        ("SUM(TRIMRANGE({TRUE,2}))", 2.0.into()),
        ("TRIMRANGE(F1:G2)", CellValue::Error(CellError::Calc)),
        ("ROWS(TRIMRANGE(F1:G2,0,0))", 2.0.into()),
        (
            "INDEX(TRIMRANGE(A1:E5),3,3)",
            CellValue::Error(CellError::NA),
        ),
    ] {
        assert_eq!(
            value(run(&b, call, 0, Default::default())),
            expected,
            "{call}"
        );
    }
}
#[test]
fn sparse_trim_observes_previously_absent_edges_and_spill_sources() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("A10").address, 4.0).unwrap();
    s.set_value(at("A12").address, 8.0).unwrap();
    s.set_formula(at("D1").address, "TRIMRANGE(A:A)").unwrap();
    s.set_formula(at("G1").address, "SEQUENCE(3)").unwrap();
    s.set_formula(at("J1").address, "TRIMRANGE(G1:G10)")
        .unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&b)
        .build(context.clone())
        .unwrap();
    assert_eq!(direct.cell(at("D3")).unwrap(), 8.0.into());
    assert_eq!(direct.cell(at("D2")).unwrap(), 0.0.into());
    assert_eq!(direct.cell(at("J3")).unwrap(), 3.0.into());
    let sparse = run(
        &trim_book(),
        "SUM(TRIMRANGE(A:G))",
        0,
        EvaluationBudgets {
            run_work: 20_000,
            materialized_values: 128,
            ..Default::default()
        },
    );
    assert_eq!(value(sparse), 14.0.into());
    let mut overlay = Overlay::with_workbook(b).build().unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D3"))
            .unwrap(),
        8.0.into()
    );
    overlay.set_value(at("A9"), 2.0).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D1"))
            .unwrap(),
        2.0.into()
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D4"))
            .unwrap(),
        8.0.into()
    );
    overlay.remove_cell(at("A12")).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D4"))
            .unwrap(),
        CellValue::Blank
    );
    overlay.set_formula(at("G1"), "SEQUENCE(2)").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("J3"))
            .unwrap(),
        CellValue::Blank
    );
}
#[test]
fn modern_profiles_stored_names_and_snapshot_roundtrip() {
    let mut b = trim_book();
    b.sheet_mut(0)
        .unwrap()
        .set_formula(at("J1").address, "_xlfn.TRIMRANGE(A1:G7)")
        .unwrap();
    b.sheet_mut(0)
        .unwrap()
        .set_formula(at("N1").address, "_xlfn.RANDARRAY(2,2)")
        .unwrap();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(b.clone()).build().unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("L3"))
            .unwrap(),
        6.0.into()
    );
    let random = overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("O2"))
        .unwrap();
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (b, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut overlay = state.attach(&b).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("O2"))
            .unwrap(),
        random
    );
    for call in ["RANDARRAY()", "TRIMRANGE({1,2})"] {
        let pinned = run_formula_with_configuration(
            &b,
            0,
            at("Z100").address,
            call,
            &RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi),
            Default::default(),
        );
        assert_eq!(value(pinned), CellValue::Error(CellError::Name));
    }
}

/// External values at absent source coordinates invalidate sparse absence proofs.
struct AbsentValueReader;
impl CachedCellReader for AbsentValueReader {
    fn cached_cell(
        &mut self,
        target: EvaluationTarget,
    ) -> Result<Option<std::sync::Arc<CellValue>>, EvaluationFailure> {
        Ok((target == at("A4")).then(|| std::sync::Arc::new(7.0.into())))
    }
}
#[test]
fn custom_reader_fallback_observes_unstored_values() {
    let mut b = book();
    b.sheet_mut(0)
        .unwrap()
        .set_formula(at("D1").address, "TRIMRANGE(A1:A5)")
        .unwrap();
    let policy = EvaluationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&b, &policy);
    let mut reader = AbsentValueReader;
    let configuration =
        RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::ScalarIntersection);
    let mut run = EvaluationRun::new_with_configuration(
        WorkbookView::new(&b, &plan, &policy).unwrap(),
        &configuration,
        Default::default(),
    )
    .unwrap()
    .with_cell_reader(&mut reader);
    assert_eq!(run.evaluate_cell(at("D1")).unwrap(), 7.0.into());
    run.finish(Ok(()));
}

#[test]
fn signed_geometry_and_lossless_array_members_keep_their_channels() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("A1").address, 2.0).unwrap();
    s.set_value(at("B2").address, 4.0).unwrap();
    s.set_value(
        at("F1").address,
        CellValue::TextUtf16(poirot_model::ExcelText::from_units(vec![0xd83d])),
    )
    .unwrap();
    // Poirot's existing signed OFFSET channel treats logical off-grid members as blanks.
    for (call, expected) in [
        ("ROWS(TRIMRANGE(OFFSET(B3,0,0,-2,-2)))", 2.0.into()),
        (
            "ROW(INDEX(TRIMRANGE(OFFSET(B3,0,0,-2,-2)),1,1))",
            1.0.into(),
        ),
        (
            "COLUMN(INDEX(TRIMRANGE(OFFSET(B3,0,0,-2,-2)),1,1))",
            1.0.into(),
        ),
        ("ROWS(TRIMRANGE(OFFSET(B3,0,0,-2,-2),0,0))", 4.0.into()),
        (
            "COLUMN(INDEX(TRIMRANGE(OFFSET(B3,0,0,-2,-2),0,0),1,1))",
            (-1.0).into(),
        ),
        (
            "INDEX(TRIMRANGE(HSTACK(F1,7)),1,1)",
            CellValue::TextUtf16(poirot_model::ExcelText::from_units(vec![0xd83d])),
        ),
    ] {
        assert_eq!(
            value(run(&b, call, 0, Default::default())),
            expected,
            "{call}"
        );
    }
    let too_small = run(
        &b,
        "IFERROR(INDEX(TRIMRANGE(HSTACK(F1,7)),1,1),9)",
        0,
        EvaluationBudgets {
            text_bytes: 1,
            ..Default::default()
        },
    );
    assert!(matches!(too_small, EvaluationOutcome::Aborted { .. }));
}
