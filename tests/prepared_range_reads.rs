//! Prepared range reads preserve worksheet semantics and operational boundaries.
use poirot::calculation::{
    EvaluationContext, EvaluationFailure, EvaluationLimits, EvaluationTarget, ProducerPolicy,
};
use poirot::eval::EvaluationPolicy;
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::eval::{
    self as eval, CellCompletion, CellObserver, EvaluationBudgets, EvaluationInputs,
    EvaluationOutcome, EvaluationRun, PreparationPolicy, WorkbookView,
};
use poirot::{CellError, CellValue, Workbook};

fn target(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

fn book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    for (address, value) in [("A1", 1.), ("A2", 2.), ("B1", 10.), ("B2", 20.)] {
        sheet.set_value(address.parse().unwrap(), value).unwrap();
    }
    for (address, formula) in [
        ("D1", "SUM(A1:A3)"),
        ("D2", "VLOOKUP(2,A1:B2,2,FALSE)"),
        ("D3", "COUNTIF(A1:A3,\">0\")"),
    ] {
        sheet
            .set_formula(address.parse().unwrap(), formula)
            .unwrap();
    }
    book
}

#[test]
fn range_consumers_keep_reference_coercion_and_overlay_dependencies() {
    let book = book();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(book.model()).build().unwrap();
    let outputs = [target("D1"), target("D2"), target("D3")];
    assert_eq!(
        overlay
            .calculation(&context)
            .unwrap()
            .outputs(&outputs)
            .unwrap()
            .into_iter()
            .map(|cell| cell.value)
            .collect::<Vec<_>>(),
        [3., 20., 2.].map(CellValue::Number)
    );
    overlay.set_value(target("A3"), 4.).unwrap();
    overlay.set_value(target("B2"), 25.).unwrap();
    assert_eq!(
        overlay
            .calculation(&context)
            .unwrap()
            .outputs(&outputs)
            .unwrap()
            .into_iter()
            .map(|cell| cell.value)
            .collect::<Vec<_>>(),
        [7., 25., 3.].map(CellValue::Number)
    );
    // Referenced booleans and numeric text are ignored by SUM, unlike direct args.
    overlay.set_value(target("A1"), true).unwrap();
    overlay.set_value(target("A2"), "2").unwrap();
    assert_eq!(
        overlay
            .calculation(&context)
            .unwrap()
            .cell(target("D1"))
            .unwrap(),
        CellValue::Number(4.)
    );
    overlay.set_formula(target("A2"), "6").unwrap();
    assert_eq!(
        overlay
            .calculation(&context)
            .unwrap()
            .cell(target("D1"))
            .unwrap(),
        CellValue::Number(10.)
    );
}

#[test]
fn literal_scans_retain_results_instead_of_copying_source_into_the_memo() {
    let book = book();
    let context = EvaluationContext {
        retention: poirot::eval::MemoRetentionLimits::default()
            .max_cached_cells(1)
            .max_logical_memo_bytes(128),
        ..Default::default()
    };
    let mut direct = Evaluation::with_workbook(book.model())
        .build_calculation(context.clone())
        .unwrap();
    assert_eq!(direct.cell(target("D1")).unwrap(), CellValue::Number(3.));
    assert_eq!(direct.retained_usage().cells, 1);
    assert_eq!(direct.retained_usage().bytes, 128);
    assert!(direct.values().all(|(key, _)| key != target("A1")));
    let mut overlay = Overlay::with_workbook(book.model()).build().unwrap();
    let mut overlay_context = context.clone();
    overlay_context.retention = overlay_context.retention.without_logical_memo_bytes_limit();
    let mut cycle = overlay.calculation(&overlay_context).unwrap();
    assert_eq!(cycle.cell(target("D1")).unwrap(), CellValue::Number(3.));
    // Sparse membership covers the absent A3 without retaining a cell result;
    // the overlay separately accounts for its actual dependency certificate.
    drop(cycle);
    assert_eq!(overlay.cached_cells(), 1);
}

#[test]
fn text_refusal_is_attributed_to_the_member_and_publication_is_atomic() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), "a literal longer than its formula")
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("D4".parse().unwrap(), "1")
        .unwrap();
    let context =
        EvaluationContext::default().limits(EvaluationLimits::default().max_text_bytes(30));
    for address in ["D1", "D2", "D3"] {
        let mut overlay = Overlay::with_workbook(book.model()).build().unwrap();
        let error = overlay
            .calculation(&context)
            .unwrap()
            .outputs(&[target("D4"), target(address)])
            .unwrap_err();
        assert_eq!(error.budget().unwrap().target, Some(target("A1")));
        assert_eq!(overlay.cached_cells(), 0);
        assert!(
            overlay
                .calculation(&EvaluationContext::default())
                .unwrap()
                .cell(target(address))
                .is_ok()
        );
    }
}

#[test]
fn numeric_admission_and_each_member_read_remain_bounded() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), f64::from_bits(1))
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_value("B2".parse().unwrap(), -0.)
        .unwrap();
    let mut direct = Evaluation::with_workbook(book.model())
        .build_calculation(EvaluationContext::default())
        .unwrap();
    assert_eq!(direct.cell(target("D1")).unwrap(), CellValue::Number(2.));
    let CellValue::Number(value) = direct.cell(target("D2")).unwrap() else {
        panic!("numeric lookup result expected")
    };
    assert_eq!(value.to_bits(), (-0.0f64).to_bits());
    book.sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), f64::INFINITY)
        .unwrap();
    let mut direct = Evaluation::with_workbook(book.model())
        .build_calculation(EvaluationContext::default())
        .unwrap();
    assert_eq!(
        direct.cell(target("D1")).unwrap(),
        CellValue::Error(CellError::Num)
    );
    let formula = "SUM(B1:B2)";
    let baseline = eval::run_formula(
        book.model(),
        0,
        target("Z1").address,
        formula,
        EvaluationInputs::default(),
        EvaluationBudgets::default(),
    );
    let work = baseline.usage().work_units;
    assert!(work >= 2);
    assert!(matches!(
        baseline,
        EvaluationOutcome::Complete {
            value: CellValue::Number(10.),
            ..
        }
    ));
    for (limit, succeeds) in [(work, true), (work - 1, false)] {
        let outcome = eval::run_formula(
            book.model(),
            0,
            target("Z1").address,
            formula,
            EvaluationInputs::default(),
            EvaluationBudgets {
                run_work: limit,
                ..Default::default()
            },
        );
        assert_eq!(
            matches!(outcome, EvaluationOutcome::Complete { .. }),
            succeeds
        );
    }
}

/// Collect all member completions to exercise the ordinary observation protocol.
#[derive(Default)]
struct Completions(Vec<EvaluationTarget>);
impl CellObserver for Completions {
    fn completed(&mut self, completion: CellCompletion<'_>) -> Result<(), EvaluationFailure> {
        self.0.push(completion.target);
        Ok(())
    }
}

#[test]
fn observer_ports_receive_every_consumed_literal_read() {
    let book = book();
    let policy = PreparationPolicy::default();
    let plan = eval::FormulaCompiler::new().compile_workbook(book.model(), &policy);
    let view = WorkbookView::new(book.model(), &plan, &policy).unwrap();
    let mut observer = Completions::default();
    let mut run = EvaluationRun::new(view, Default::default(), Default::default())
        .unwrap()
        .with_cell_observer(&mut observer);
    assert_eq!(
        run.evaluate_cell(target("D1")).unwrap(),
        CellValue::Number(3.)
    );
    assert_eq!(
        run.evaluate_cell(target("D2")).unwrap(),
        CellValue::Number(20.)
    );
    drop(run);
    for address in ["A1", "A2", "B2", "D1", "D2"] {
        assert!(observer.0.contains(&target(address)), "{address}");
    }
}

#[test]
fn saved_and_dynamic_spill_members_keep_ownership_and_formula_semantics() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    sheet.set_value("E1".parse().unwrap(), 2.).unwrap();
    sheet
        .set_formula_with_cached_value("A1".parse().unwrap(), "SEQUENCE(E1)", 10.0.into())
        .unwrap();
    sheet
        .set_spill_extent("A1".parse().unwrap(), "A2".parse().unwrap(), "SEQUENCE(E1)")
        .unwrap();
    sheet
        .set_spill_cached_value(
            "A2".parse().unwrap(),
            "A1".parse().unwrap(),
            "SEQUENCE(E1)",
            20.0.into(),
        )
        .unwrap();
    sheet
        .set_formula("C1".parse().unwrap(), "SUM(A1:A3)")
        .unwrap();
    sheet
        .set_formula("C2".parse().unwrap(), "VLOOKUP(2,A1:A3,1,FALSE)")
        .unwrap();
    for (policy, expected) in [
        (ProducerPolicy::Compute, 3.),
        (ProducerPolicy::TrustSaved, 30.),
    ] {
        let mut overlay = Overlay::with_workbook(book.model())
            .with_policy(EvaluationPolicy {
                producers: policy,
                ..Default::default()
            })
            .build()
            .unwrap();
        let context = EvaluationContext::default();
        assert_eq!(
            overlay
                .calculation(&context)
                .unwrap()
                .cell(target("C1"))
                .unwrap(),
            CellValue::Number(expected)
        );
        if policy == ProducerPolicy::Compute {
            assert_eq!(
                overlay
                    .calculation(&context)
                    .unwrap()
                    .cell(target("C2"))
                    .unwrap(),
                CellValue::Number(2.)
            );
            overlay.set_value(target("E1"), 3.).unwrap();
            assert_eq!(
                overlay
                    .calculation(&context)
                    .unwrap()
                    .cell(target("C1"))
                    .unwrap(),
                CellValue::Number(6.)
            );
            overlay.set_value(target("E1"), 1.).unwrap();
            assert_eq!(
                overlay
                    .calculation(&context)
                    .unwrap()
                    .cell(target("C1"))
                    .unwrap(),
                CellValue::Number(1.)
            );
        }
    }
}

#[test]
fn depth_and_retention_refusals_do_not_publish_partial_range_results() {
    let book = book();
    for context in [
        EvaluationContext::default().limits(EvaluationLimits::default().max_depth(1)),
        EvaluationContext {
            retention: poirot::eval::MemoRetentionLimits::default().max_cached_cells(0),
            ..Default::default()
        },
    ] {
        let mut overlay = Overlay::with_workbook(book.model()).build().unwrap();
        assert!(
            overlay
                .calculation(&context)
                .unwrap()
                .cell(target("D1"))
                .is_err()
        );
        assert_eq!(overlay.cached_cells(), 0);
        let mut direct = Evaluation::with_workbook(book.model())
            .build_calculation(context)
            .unwrap();
        assert!(direct.cell(target("D1")).is_err());
        assert_eq!(direct.retained_usage().cells, 0);
        assert_eq!(direct.values().count(), 0);
    }
}

/// Caller-validated source override used to verify the external cache read boundary.
struct ExternalValue;
impl eval::CachedCellReader for ExternalValue {
    fn cached_cell(
        &mut self,
        key: EvaluationTarget,
    ) -> Result<Option<std::sync::Arc<CellValue>>, EvaluationFailure> {
        Ok((key == target("A1")).then(|| std::sync::Arc::new(CellValue::Number(100.))))
    }
}

#[test]
fn external_readers_still_override_authored_range_members() {
    let book = book();
    let policy = PreparationPolicy::default();
    let plan = eval::FormulaCompiler::new().compile_workbook(book.model(), &policy);
    let view = WorkbookView::new(book.model(), &plan, &policy).unwrap();
    let mut reader = ExternalValue;
    let mut run = EvaluationRun::new(view, Default::default(), Default::default())
        .unwrap()
        .with_cell_reader(&mut reader);
    assert_eq!(
        run.evaluate_cell(target("D1")).unwrap(),
        CellValue::Number(102.)
    );
}
