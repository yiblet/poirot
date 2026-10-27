//! Ordered SUM follows sequential binary64 arithmetic and ignores reference text/logicals.
//! https://support.microsoft.com/en-us/excel/functions/sum-function
//! Final subnormals become zero under the documented numeric fidelity policy:
//! https://learn.microsoft.com/en-us/troubleshoot/microsoft-365-apps/excel/floating-point-arithmetic-inaccurate-result
//! Geometry, work admission, lifetime and spill rejection are Poirot API contracts.
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::eval::*;
use poirot::model::{CellValue, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

fn book(values: &[(&str, CellValue)], formulas: &[(&str, &str)]) -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (address, value) in values {
        sheet.set_value(at(address).address, value.clone()).unwrap();
    }
    for (address, formula) in formulas {
        sheet.set_formula(at(address).address, *formula).unwrap();
    }
    book
}

#[test]
fn raw_subnormal_accumulation_survives_final_result_normalization() {
    // All source values are normal. Exact subtraction leaves the smallest
    // subnormal; normalizing an intermediate sum would lose one ULP.
    let first = 2.0 * f64::MIN_POSITIVE;
    let second = -f64::from_bits(first.to_bits() - 1);
    let third = f64::MIN_POSITIVE;
    let raw = (0.0 + first) + second;
    assert_eq!(raw.to_bits(), 1);
    let expected = (raw + third).to_bits();
    assert_eq!(expected, f64::MIN_POSITIVE.to_bits() + 1);
    let book = book(
        &[
            ("A1", first.into()),
            ("A2", second.into()),
            ("A3", third.into()),
        ],
        &[("Z1", "SUM(A1:A2)"), ("Z2", "SUM(A1:A3)")],
    );
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let mut incremental = overlay.evaluation(&context).unwrap();
    for target in [at("Z1"), at("Z2")] {
        for value in [
            direct.cell(target).unwrap(),
            incremental.cell(target).unwrap(),
        ] {
            let CellValue::Number(value) = value else {
                panic!("numeric result required")
            };
            assert_eq!(
                value.to_bits(),
                if target == at("Z1") { 0 } else { expected }
            );
        }
    }
    direct.finish();
    incremental.finish();
}

#[test]
fn range_shapes_holes_seeds_and_operations_preserve_sequential_results() {
    let book = book(
        &[
            ("A1", 1e16.into()),
            ("B1", 1.0.into()),
            ("A2", (-1e16).into()),
            ("B2", 2.0.into()),
            ("A3", 4.0.into()),
            ("C1", 3.0.into()),
            ("C2", true.into()),
        ],
        &[
            ("Z1", "SUM(A1:B1)"),
            ("Z2", "SUM(A1:B2)"),
            ("Z3", "SUM(A1:B3)"),
            ("Z4", "SUM(A1:C3)"),
            ("Z5", "SUM(A1:C1)"),
            ("Z6", "SUM(A1:C1)"),
            ("Z7", "SUM(1,A1:C1)"),
            ("Z8", "SUM(2,A1:C1)"),
            ("Z9", "MAX(A1:C1)"),
            ("Z10", "MIN(A1:C1)"),
            ("Z11", "SUM(A2:C2)"),
            ("Z12", "SUM(A1:A1)"),
            ("Z13", "SUMSQ(A1:A1)"),
            ("Z14", "AVERAGE(A1:A2)"),
            ("Z15", "PRODUCT(B1:B1)"),
            ("Z16", "PRODUCT(B1:B2)"),
        ],
    );
    let expected = [
        1e16,
        2.,
        6.,
        10.,
        1e16 + 4.,
        1e16 + 4.,
        1e16 + 4.,
        1e16 + 8.,
        1e16,
        1.,
        -1e16 + 2.,
        1e16,
        1e32,
        0.,
        1.,
        2.,
    ];
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let mut incremental = overlay.evaluation(&context).unwrap();
    for (i, expected) in expected.into_iter().enumerate() {
        let target = at(&format!("Z{}", i + 1));
        assert_eq!(
            direct.cell(target).unwrap(),
            CellValue::Number(expected),
            "direct {i}"
        );
        assert_eq!(
            incremental.cell(target).unwrap(),
            CellValue::Number(expected),
            "overlay {i}"
        );
    }
}

#[test]
fn direct_edits_resets_begin_clear_policy_and_snapshot_start_correct_state() {
    let book = book(
        &[("A1", 1.0.into()), ("A2", 2.0.into()), ("A3", 3.0.into())],
        &[("Z1", "SUM(A1:A2)"), ("Z2", "SUM(A1:A3)")],
    );
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(direct.cell(at("Z1")).unwrap(), 3.0.into());
    direct.set_value(at("A1"), 10.).unwrap();
    assert_eq!(direct.cell(at("Z2")).unwrap(), 15.0.into());
    direct.reset();
    assert_eq!(direct.cell(at("Z1")).unwrap(), 3.0.into());
    direct.clear_cache();
    assert_eq!(direct.cell(at("Z2")).unwrap(), 6.0.into());
    direct.begin(Default::default()).unwrap();
    assert_eq!(direct.cell(at("Z1")).unwrap(), 3.0.into());
    let mut policy = direct.policy().clone();
    policy.arrays = ArrayResultPolicy::ScalarIntersection;
    direct.set_policy(policy).unwrap();
    assert_eq!(direct.cell(at("Z2")).unwrap(), 6.0.into());
    let snapshot = postcard::to_allocvec(&direct.snapshot()).unwrap();
    let (baseline, state) =
        Evaluation::from_snapshot(postcard::from_bytes(&snapshot).unwrap()).unwrap();
    let mut restored = state
        .into_evaluation(&baseline, Default::default())
        .unwrap();
    assert_eq!(restored.cell(at("Z2")).unwrap(), 6.0.into());
    assert_eq!(restored.finish().outcome.usage().cell_reads, 4);
}

#[test]
fn growing_members_enforce_conceptual_collection_limit_at_first_new_overflow() {
    let mut book = book(&[], &[("Z1", "SUM(A1:C3)"), ("Z2", "SUM(A1:C5)")]);
    let sheet = book.sheet_mut(0).unwrap();
    for index in (0..7).chain(9..15) {
        sheet
            .set_value(
                poirot::model::CellAddress {
                    row: index / 3,
                    col: index % 3,
                },
                1.,
            )
            .unwrap();
    }
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        // A narrow scalar policy isolates the operand limit. Dynamic discovery
        // also fits this source layout, so it exercises the same finite guard.
        for warm in [false, true] {
            let mut direct = Evaluation::with_workbook(&book)
                .with_policy(poirot::eval::EvaluationPolicy {
                    arrays,
                    ..Default::default()
                })
                .build(EvaluationContext {
                    limits: poirot::eval::EvaluationLimits::default().max_collection_values(8),
                    ..Default::default()
                })
                .unwrap();
            if warm {
                assert_eq!(direct.cell(at("Z1")).unwrap(), 7.0.into());
            }
            assert!(matches!(
                direct.cell(at("Z2")),
                Err(EvaluationFailure::Budget(BudgetFailure {
                    kind: BudgetKind::Materialization,
                    requested: 9,
                    ..
                }))
            ));
            assert!(direct.values().all(|(target, _)| target != at("Z2")));
        }
    }
}

#[test]
fn horizontal_identical_and_vertical_ranges_have_ordered_results() {
    for formulas in [
        [
            ("Z1", "SUM(A1:B1)"),
            ("Z2", "SUM(A1:C1)"),
            ("Z3", "SUM(A1:C1)"),
        ],
        [
            ("Z1", "SUM(A1:A2)"),
            ("Z2", "SUM(A1:A3)"),
            ("Z3", "SUM(A1:A3)"),
        ],
    ] {
        let book = book(
            &[
                ("A1", 1.0.into()),
                ("B1", 2.0.into()),
                ("C1", 3.0.into()),
                ("A2", 2.0.into()),
                ("A3", 3.0.into()),
            ],
            &formulas,
        );
        let context = EvaluationContext::default();
        let mut direct = Evaluation::with_workbook(&book)
            .build(context.clone())
            .unwrap();
        let mut overlay = Overlay::with_workbook(&book).build().unwrap();
        let mut incremental = overlay.evaluation(&context).unwrap();
        for (target, expected) in [("Z1", 3.), ("Z2", 6.), ("Z3", 6.)] {
            assert_eq!(direct.cell(at(target)).unwrap(), expected.into());
            assert_eq!(incremental.cell(at(target)).unwrap(), expected.into());
        }
        direct.finish();
        incremental.finish();
    }
}

#[test]
fn nested_reductions_and_errors_never_publish_partial_operands() {
    let book = book(
        &[("A1", 1.0.into()), ("D1", 2.0.into()), ("D2", 3.0.into())],
        &[
            ("A2", "SUM(D1:D2)"),
            ("Z1", "SUM(A1:A1)"),
            ("Z2", "SUM(A1:A2)"),
            ("Z3", "SUM(A1:A3)"),
            ("Z4", "SUM(A1:A3,#REF!)"),
            ("Z5", "SUM(A1:A4)"),
            ("Z6", "SUM(A1:A4)/0"),
            ("Z7", "SUM(A1:A5)"),
        ],
    );
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    for (target, expected) in [
        ("Z1", 1.0.into()),
        ("Z2", 6.0.into()),
        ("Z3", 6.0.into()),
        ("Z4", CellValue::Error(poirot::model::CellError::Ref)),
        ("Z5", 6.0.into()),
        ("Z6", CellValue::Error(poirot::model::CellError::Div0)),
        ("Z7", 6.0.into()),
    ] {
        assert_eq!(direct.cell(at(target)).unwrap(), expected);
    }
}

#[test]
fn visiting_spill_producer_preserves_sparse_aggregation() {
    let book = book(&[], &[("A1", "SUM(B1:B2)+SUM(B1:B3)+{1,2}")]);
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let mut incremental = overlay.evaluation(&context).unwrap();
    for result in [direct.cell(at("A1")), incremental.cell(at("A1"))] {
        assert!(
            matches!(result, Err(EvaluationFailure::UnsupportedCapability { detail, .. })
            if detail.contains("own unpublished sparse footprint"))
        );
    }
    assert!(direct.changes().is_empty());
}

#[test]
fn spill_discovery_before_lookup_restores_outside_owners_and_tracks_blank_insertions() {
    let book = book(
        &[("A1", 5.0.into())],
        &[
            ("B3", "{7,8}"),
            ("Z1", "SUM(C1:C2)"),
            ("Z2", "SUM(C1:C3)"),
            ("Z3", "SUM(C1:C4)"),
        ],
    );
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    {
        let mut incremental = overlay.evaluation(&context).unwrap();
        for (target, expected) in [("Z1", 0.), ("Z2", 8.), ("Z3", 8.)] {
            assert_eq!(direct.cell(at(target)).unwrap(), expected.into());
            assert_eq!(incremental.cell(at(target)).unwrap(), expected.into());
        }
    }
    // A new output must restore the cached footprint when its outside owner is
    // retained, rather than treating the source's absent C3 as blank.
    overlay.set_formula(at("Z4"), "SUM(C1:C5)").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("Z4"))
            .unwrap(),
        8.0.into()
    );
    overlay.set_value(at("C2"), 2.).unwrap();
    direct.set_value(at("C2"), 2.).unwrap();
    for target in ["Z1", "Z2", "Z3"] {
        let expected = if target == "Z1" { 2. } else { 10. };
        assert_eq!(direct.cell(at(target)).unwrap(), expected.into());
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at(target))
                .unwrap(),
            expected.into()
        );
    }
    overlay.set_value(at("C3"), 99.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("Z3"))
            .unwrap(),
        101.0.into()
    );
}

#[test]
fn full_preparation_supports_named_dynamic_producers() {
    let mut book = book(&[], &[("Z1", "SUM(B1:B1)"), ("Z2", "SUM(B1:B2)")]);
    book.define_name("Producer", "{1,2}", None).unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("A2").address, "Producer")
        .unwrap();
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(direct.cell(at("Z1")).unwrap(), 0.0.into());
    assert_eq!(direct.cell(at("Z2")).unwrap(), 2.0.into());
}

#[test]
fn aggregations_track_context_volatile_and_nested_source_changes() {
    use std::time::{Duration, UNIX_EPOCH};
    let book = book(
        &[("D1", 2.0.into())],
        &[
            ("A1", "NOW()"),
            ("A2", "D1*2"),
            ("Z1", "SUM(A1:A1)"),
            ("Z2", "SUM(A1:A2)"),
            ("Z3", "SUM(A1:A3)"),
        ],
    );
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let mut context = EvaluationContext {
        as_of_time: UNIX_EPOCH + Duration::from_secs(1_609_502_400),
        ..Default::default()
    };
    let mut previous = None;
    for day in 0..3 {
        if day > 0 {
            context.as_of_time += Duration::from_secs(86400);
        }
        if day == 2 {
            overlay.set_value(at("D1"), 3.).unwrap();
        }
        let mut run = overlay.evaluation(&context).unwrap();
        let CellValue::Number(now) = run.cell(at("Z1")).unwrap() else {
            panic!("numeric NOW")
        };
        if let Some(previous) = previous {
            assert_eq!(now, previous + 1.);
        }
        previous = Some(now);
        let expected = now + if day == 2 { 6. } else { 4. };
        assert_eq!(run.cell(at("Z2")).unwrap(), expected.into());
        assert_eq!(run.cell(at("Z3")).unwrap(), expected.into());
    }
}

#[test]
fn ignored_reference_text_and_later_text_limits_preserve_errors() {
    for text in [
        CellValue::Text("x".repeat(32)),
        CellValue::TextUtf16(poirot::model::ExcelText::from_units(vec![0xd800; 16])),
    ] {
        let book = book(
            &[("A1", 1.0.into()), ("A2", "ok".into()), ("A3", text)],
            &[("Z1", "SUM(A1:A2)"), ("Z2", "SUM(A1:A3)")],
        );
        let mut direct = Evaluation::with_workbook(&book)
            .build(EvaluationContext {
                limits: poirot::eval::EvaluationLimits::default().max_text_bytes(16),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(direct.cell(at("Z1")).unwrap(), 1.0.into());
        assert!(
            matches!(direct.cell(at("Z2")), Err(EvaluationFailure::Budget(BudgetFailure {
            kind: BudgetKind::TextBytes, target: Some(target), ..
        })) if target == at("A3"))
        );
    }
}

#[test]
fn operational_member_failure_wins_over_earlier_spreadsheet_error() {
    let mut book = book(
        &[
            ("A1", 1.0.into()),
            ("A2", CellValue::Error(poirot::model::CellError::Div0)),
        ],
        &[
            ("Z1", "SUM(A1:A1)"),
            ("Z2", "SUM(A1:A3)"),
            ("A3", "SUM(U:T!B1)"),
        ],
    );
    book.add_sheet("T").unwrap();
    book.add_sheet("U").unwrap();
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(direct.cell(at("Z1")).unwrap(), 1.0.into());
    assert!(matches!(
        direct.cell(at("Z2")),
        Err(EvaluationFailure::UnsupportedCapability { .. })
    ));
    assert!(direct.values().all(|(target, _)| target != at("Z2")));
}

#[test]
fn failed_overlay_retention_never_publishes_a_partial_batch() {
    let book = book(
        &[("A1", 1.0.into()), ("A2", 2.0.into()), ("A3", 3.0.into())],
        &[("Z1", "SUM(A1:A2)"), ("Z2", "SUM(A1:A3)")],
    );
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let mut context = EvaluationContext::default();
    // Literal range members need no memo entries; admit only the first formula.
    context.retention.cells = 1;
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at("Z1")).unwrap(), 3.0.into());
        assert!(matches!(
            run.cell(at("Z2")),
            Err(EvaluationFailure::MemoRetentionLimit { .. })
        ));
    }
    assert!(overlay.cached_value(at("Z1")).is_some());
    assert!(overlay.cached_value(at("Z2")).is_none());
    assert!(overlay.cached_value(at("A3")).is_none());
}

/// Capture complete values and order at every shared cell read, including memo hits.
#[derive(Default)]
struct Completions(Vec<(EvaluationTarget, CellValue)>);
impl CellObserver for Completions {
    fn completed(&mut self, event: CellCompletion<'_>) -> Result<(), EvaluationFailure> {
        self.0
            .push((event.target, event.result.cloned().map_err(Clone::clone)?));
        Ok(())
    }
}

#[test]
fn observer_fallback_preserves_exact_repeated_full_text_payloads_and_order() {
    for text in [
        CellValue::Text("x".repeat(32)),
        CellValue::TextUtf16(poirot::model::ExcelText::from_units(vec![0xd800; 16])),
    ] {
        let book = book(
            &[("A1", text.clone()), ("A2", 2.0.into()), ("A3", 3.0.into())],
            &[("Z1", "SUM(A1:A2)"), ("Z2", "SUM(A1:A3)")],
        );
        let policy = PreparationPolicy::default();
        let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
        let mut observed = Completions::default();
        let mut run = EvaluationRun::new(
            WorkbookView::new(&book, &plan, &policy).unwrap(),
            Default::default(),
            Default::default(),
        )
        .unwrap()
        .with_cell_observer(&mut observed);
        assert_eq!(run.evaluate_cell(at("Z1")).unwrap(), 2.0.into());
        assert_eq!(run.evaluate_cell(at("Z2")).unwrap(), 5.0.into());
        assert_eq!(run.finish(Ok(())).outcome.usage().cell_reads, 7);
        assert_eq!(
            observed.0,
            vec![
                (at("A1"), text.clone()),
                (at("A2"), 2.0.into()),
                (at("Z1"), 2.0.into()),
                (at("A1"), text),
                (at("A2"), 2.0.into()),
                (at("A3"), 3.0.into()),
                (at("Z2"), 5.0.into())
            ]
        );
    }
}

/// Model a legal nonmemoized external reader whose result changes on each access.
#[derive(Default)]
struct ChangingReader {
    reads: u32,
}
impl CachedCellReader for ChangingReader {
    fn cached_cell(
        &mut self,
        target: EvaluationTarget,
    ) -> Result<Option<std::sync::Arc<CellValue>>, EvaluationFailure> {
        if target.address.col == 0 {
            self.reads += 1;
            Ok(Some(std::sync::Arc::new(CellValue::Number(
                self.reads as f64,
            ))))
        } else {
            Ok(None)
        }
    }
}

#[test]
fn external_reader_observes_each_operand_in_original_order() {
    let book = book(
        &[("A1", 0.0.into()), ("A2", 0.0.into())],
        &[("Z1", "SUM(A1:A1)"), ("Z2", "SUM(A1:A2)")],
    );
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
    let mut reader = ChangingReader::default();
    let mut run = EvaluationRun::new(
        WorkbookView::new(&book, &plan, &policy).unwrap(),
        Default::default(),
        Default::default(),
    )
    .unwrap()
    .with_cell_reader(&mut reader);
    assert_eq!(run.evaluate_cell(at("Z1")).unwrap(), 1.0.into());
    assert_eq!(run.evaluate_cell(at("Z2")).unwrap(), 5.0.into());
    run.finish(Ok(()));
    assert_eq!(reader.reads, 3);
}

#[test]
fn range_member_can_admit_an_unrelated_spill() {
    let book = book(
        &[("A1", 1.0.into())],
        &[
            ("A2", "D2"),
            ("D2", "{2,3}"),
            ("Z1", "SUM(A1:A1)"),
            ("Z2", "SUM(A1:A2)"),
            ("Z3", "SUM(A1:A3)"),
        ],
    );
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let mut run = overlay.evaluation(&context).unwrap();
    for (target, expected) in [("Z1", 1.), ("Z2", 3.), ("Z3", 3.), ("E2", 3.)] {
        assert_eq!(direct.cell(at(target)).unwrap(), expected.into());
        assert_eq!(run.cell(at(target)).unwrap(), expected.into());
    }
}

#[test]
fn blank_spill_children_are_restored_and_never_hide_later_source_insertions() {
    let book = book(
        &[("G1", 2.0.into())],
        &[
            ("D1", "TRANSPOSE(G1:G2)"),
            ("Z1", "SUM(E1:E1)"),
            ("Z2", "SUM(E1:E2)"),
        ],
    );
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at("Z1")).unwrap(), 0.0.into());
        assert_eq!(run.cell(at("Z2")).unwrap(), 0.0.into());
    }
    overlay.set_formula(at("Z3"), "SUM(E1:E3)").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("Z3"))
            .unwrap(),
        0.0.into()
    );
    overlay.set_value(at("E1"), 4.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("Z3"))
            .unwrap(),
        4.0.into()
    );
}

#[test]
fn all_six_numeric_operations_preserve_order_through_the_public_prepared_run() {
    let mut book = book(
        &[("A1", 2.0.into()), ("A2", 4.0.into()), ("A3", 4.0.into())],
        &[],
    );
    let operations = [
        ("SUM", 6., 10.),
        ("SUMSQ", 20., 36.),
        ("AVERAGE", 3., 10. / 3.),
        ("MIN", 2., 2.),
        ("MAX", 4., 4.),
        ("PRODUCT", 8., 32.),
    ];
    for (index, (name, _, _)) in operations.iter().enumerate() {
        for extra in 0..2 {
            book.sheet_mut(0)
                .unwrap()
                .set_formula(
                    at(&format!("Z{}", index * 2 + extra + 1)).address,
                    format!("{name}(A1:A{})", extra + 2),
                )
                .unwrap();
        }
    }
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
    let mut run = EvaluationRun::new(
        WorkbookView::new(&book, &plan, &policy).unwrap(),
        Default::default(),
        Default::default(),
    )
    .unwrap();
    for (index, (_, first, second)) in operations.iter().enumerate() {
        for (extra, want) in [*first, *second].into_iter().enumerate() {
            assert_eq!(
                run.evaluate_cell(at(&format!("Z{}", index * 2 + extra + 1)))
                    .unwrap(),
                CellValue::Number(want)
            );
        }
    }
    run.finish(Ok(()));
}
