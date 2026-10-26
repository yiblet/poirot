//! Original examples derived from Microsoft's documented dynamic-array and blocker behavior:
//! https://support.microsoft.com/en-us/excel/dynamic-array-formulas-and-spilled-array-behavior
//! Strict top-left priority and ownership settlement are Poirot policies.
use poirot::eval::EvaluationTarget;
use poirot::eval::overlay::{Evaluation, EvaluationContext, Overlay};
use poirot::model::{CellError, CellValue, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
fn book(formulas: &[(&str, &str)]) -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for &(address, formula) in formulas {
        sheet.set_formula(at(address).address, formula).unwrap();
    }
    book
}
fn verify(book: &Workbook, expected: &[(&str, CellValue)]) {
    for reverse in [false, true] {
        let mut direct = Evaluation::with_workbook(book)
            .build(Default::default())
            .unwrap();
        let mut overlay = Overlay::with_workbook(book).build().unwrap();
        let context = EvaluationContext::default();
        let mut expected = expected.to_vec();
        if reverse {
            expected.reverse();
        }
        for _ in 0..2 {
            let mut evaluation = overlay.evaluation(&context).unwrap();
            for (address, value) in &expected {
                assert_eq!(
                    direct.cell(at(address)).unwrap(),
                    *value,
                    "direct {address}"
                );
                assert_eq!(
                    evaluation.cell(at(address)).unwrap(),
                    *value,
                    "overlay {address}"
                );
            }
        }
    }
}
#[test]
fn actual_later_child_dependency_with_disjoint_footprints_succeeds() {
    let book = book(&[("D1", "{1,2}+A6"), ("A5", "{10;20}")]);
    verify(
        &book,
        &[
            ("D1", 21.0.into()),
            ("E1", 22.0.into()),
            ("A6", 20.0.into()),
        ],
    );
}
#[test]
fn potential_owner_excluding_read_stays_held_and_preserves_canonical_overlap() {
    // D1:D5 and A5:D6 overlap at D5; A7 is outside the actual later footprint.
    let book = book(&[
        ("D1", "{1;2;3;4;5}+SUM(A7)"),
        ("A5", "{10,11,12,13;20,21,22,23}"),
    ]);
    verify(
        &book,
        &[
            ("D5", 5.0.into()),
            ("A5", CellValue::Error(CellError::Spill)),
            ("A6", CellValue::Blank),
        ],
    );
}
#[test]
fn independent_incomparable_anchors_preserve_canonical_overlap_priority() {
    let book = book(&[("D1", "{1;2;3;4;5}"), ("A5", "{10,11,12,13;20,21,22,23}")]);
    verify(
        &book,
        &[
            ("D5", 5.0.into()),
            ("A5", CellValue::Error(CellError::Spill)),
            ("A6", CellValue::Blank),
        ],
    );
}
#[test]
fn top_left_winner_recomputes_after_its_supplier_loses() {
    let book = book(&[
        ("D1", "{1;2;3;4;5}+A6"),
        ("A5", "{10,11,12,13;20,21,22,23}"),
    ]);
    verify(
        &book,
        &[
            ("D1", 1.0.into()),
            ("D5", 5.0.into()),
            ("A5", CellValue::Error(CellError::Spill)),
            ("A6", CellValue::Blank),
            ("D6", CellValue::Blank),
        ],
    );
}
#[test]
fn scalar_intermediates_and_cross_sheet_producers_read_final_children() {
    let mut book = book(&[("D1", "{1,2}+B1"), ("B1", "SUM(A6)"), ("A5", "{10;20}")]);
    let other = book.add_sheet("Other").unwrap();
    other
        .set_formula("A1".parse().unwrap(), "TRANSPOSE(S!D1:E1)")
        .unwrap();
    verify(&book, &[("E1", 22.0.into()), ("B1", 20.0.into())]);
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(
        direct
            .cell(EvaluationTarget {
                sheet: 1,
                address: "A2".parse().unwrap()
            })
            .unwrap(),
        22.0.into()
    );
}
#[test]
fn ordinary_sheet_reads_do_not_prepare_unreferenced_sheets() {
    let mut book = book(&[("A1", "{1,2}")]);
    book.add_sheet("Other")
        .unwrap()
        .set_formula("A1".parse().unwrap(), "{1,2}+SUM(A1:B2)")
        .unwrap();
    verify(&book, &[("B1", 2.0.into())]);
}
#[test]
fn named_dynamic_sizes_grow_shrink_and_release_complete_footprints() {
    let mut book = book(&[("A1", "Choice"), ("D1", "SUM(B1:B2)")]);
    book.define_name("Choice", "IF(F1,{1,2;3,4},9)", None)
        .unwrap();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let context = EvaluationContext::default();
    for (choice, sum, child) in [
        (false, 0.0, CellValue::Blank),
        (true, 6.0, 4.0.into()),
        (false, 0.0, CellValue::Blank),
    ] {
        overlay.set_value(at("F1"), choice).unwrap();
        let mut evaluation = overlay.evaluation(&context).unwrap();
        assert_eq!(evaluation.cell(at("D1")).unwrap(), sum.into());
        assert_eq!(evaluation.cell(at("B2")).unwrap(), child);
    }
}
#[test]
fn generated_search_keys_and_return_columns_are_visible_to_lookup() {
    let book = book(&[
        ("A1", "{1,10;2,20;3,30}"),
        ("D1", "VLOOKUP(2,A1:B3,2,FALSE)"),
        ("D2", "MATCH(3,A1:A3,0)"),
        ("D3", "VLOOKUP(8,A1:B3,2,FALSE)"),
    ]);
    verify(
        &book,
        &[
            ("D1", 20.0.into()),
            ("D2", 3.0.into()),
            ("D3", CellValue::Error(CellError::NA)),
        ],
    );
}
#[test]
fn circular_blank_and_sparse_footprints_abort_without_external_publication() {
    for formula in ["{1,2}+B1", "{1,2}+SUM(B1:B2)"] {
        let book = book(&[("A1", formula), ("D1", "{7,8}")]);
        let mut overlay = Overlay::with_workbook(&book).build().unwrap();
        let context = EvaluationContext::default();
        let mut evaluation = overlay.evaluation(&context).unwrap();
        assert_eq!(evaluation.cell(at("E1")).unwrap(), 8.0.into());
        let before: Vec<_> = evaluation.values().map(|(t, v)| (t, v.clone())).collect();
        assert!(evaluation.cell(at("B1")).is_err());
        let after: Vec<_> = evaluation.values().map(|(t, v)| (t, v.clone())).collect();
        assert_eq!(before, after);
        assert!(evaluation.cell(at("D1")).is_err());
    }
}
#[test]
fn held_producer_and_scalar_dependency_do_not_escape_a_later_phase_failure() {
    let book = book(&[
        ("D1", "{1;2;3;4;5}+SUM(A7)"),
        ("A5", "{10,11,12,13;20,21,22,23}"),
        ("J1", "{1,2}+K1"),
    ]);
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(at("D5")).unwrap(), 5.0.into());
    let before: Vec<_> = evaluation.values().map(|(t, v)| (t, v.clone())).collect();
    assert!(evaluation.cell(at("K1")).is_err());
    let after: Vec<_> = evaluation.values().map(|(t, v)| (t, v.clone())).collect();
    assert_eq!(before, after);
}
#[test]
fn unchanged_overlay_source_reuses_candidate_classification_after_value_edits() {
    let mut book = book(&[("A1", "IF(F1,{1,2},9)"), ("J1", "1+1")]);
    for row in 2..=129 {
        book.sheet_mut(0)
            .unwrap()
            .set_formula(format!("J{row}").parse().unwrap(), format!("{row}+1"))
            .unwrap();
    }
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("B1"))
        .unwrap();
    let cold = overlay.compiled_usage().parse_attempts;
    // Only the producer is inside the B1 demand and its F1 dependency prefixes.
    // The scalar formulas in J are not classified until a later demand reaches them.
    assert_eq!(cold, 1);
    for value in [true, false, true] {
        let previous = overlay.compiled_usage().reused_formulas;
        overlay.set_value(at("F1"), value).unwrap();
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B1"))
            .unwrap();
        let usage = overlay.compiled_usage();
        assert_eq!(usage.parse_attempts, cold);
        assert!(
            usage.reused_formulas - previous <= 2,
            "the candidate index must not requery every scalar formula"
        );
    }
}
#[test]
fn generated_lookup_hit_and_miss_invalidate_when_a_former_blank_gains_an_earlier_key() {
    let book = book(&[
        ("A3", "{2,30;3,40}"),
        ("D1", "VLOOKUP(2,A1:B4,2,FALSE)"),
        ("D2", "VLOOKUP(1,A1:B4,2,FALSE)"),
    ]);
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let context = EvaluationContext::default();
    {
        let mut e = overlay.evaluation(&context).unwrap();
        assert_eq!(e.cell(at("D1")).unwrap(), 30.0.into());
        assert_eq!(e.cell(at("D2")).unwrap(), CellValue::Error(CellError::NA));
    }
    overlay.set_formula(at("A1"), "{1,10;2,20}").unwrap();
    for _ in 0..2 {
        let mut e = overlay.evaluation(&context).unwrap();
        assert_eq!(e.cell(at("D1")).unwrap(), 20.0.into());
        assert_eq!(e.cell(at("D2")).unwrap(), 10.0.into());
    }
    overlay.remove_cell(at("A1")).unwrap();
    let mut e = overlay.evaluation(&context).unwrap();
    assert_eq!(e.cell(at("D1")).unwrap(), 30.0.into());
    assert_eq!(e.cell(at("D2")).unwrap(), CellValue::Error(CellError::NA));
}
#[test]
fn producer_observers_receive_finalized_results_and_ordinary_child_completions() {
    use poirot::eval::*;
    /// Capture final cell results without retaining a borrow into evaluation.
    #[derive(Default)]
    struct Observer(Vec<(EvaluationTarget, CellProvenance, CellValue)>);
    impl CellObserver for Observer {
        fn completed(&mut self, event: CellCompletion<'_>) -> Result<(), EvaluationFailure> {
            if let Ok(value) = event.result {
                self.0.push((event.target, event.provenance, value.clone()));
            }
            Ok(())
        }
    }
    let book = book(&[("A1", "{10,20}"), ("D1", "{1,2}+B1")]);
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
    let mut observer = Observer::default();
    let mut run = EvaluationRun::new(
        WorkbookView::new(&book, &plan, &policy).unwrap(),
        Default::default(),
        Default::default(),
    )
    .unwrap()
    .with_cell_observer(&mut observer);
    assert_eq!(run.evaluate_cell(at("E1")).unwrap(), 22.0.into());
    run.finish(Ok(()));
    assert!(
        observer
            .0
            .contains(&(at("A1"), CellProvenance::Evaluated, 10.0.into()))
    );
    assert!(
        observer
            .0
            .contains(&(at("D1"), CellProvenance::Evaluated, 21.0.into()))
    );
    assert!(
        observer
            .0
            .contains(&(at("B1"), CellProvenance::SessionMemo, 20.0.into()))
    );
    assert!(
        observer
            .0
            .contains(&(at("E1"), CellProvenance::SessionMemo, 22.0.into()))
    );
}
#[test]
fn a_dirty_growing_producer_cannot_consume_a_stale_scalar_in_its_own_new_footprint() {
    let mut book = book(&[("D1", "Choice"), ("E5", "SUM(D5)")]);
    book.define_name("Choice", "IF(J1,{1;2;3;4;5}+E5,9)", None)
        .unwrap();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("E5"))
            .unwrap(),
        0.0.into()
    );
    overlay.set_value(at("J1"), true).unwrap();
    assert!(overlay.cached_value(at("E5")).is_none());
    let mut e = overlay.evaluation(&context).unwrap();
    assert!(e.cell(at("D5")).is_err());
    assert_eq!(e.values().count(), 0);
    assert!(e.result_changes().unwrap().is_empty());
}
#[test]
fn every_consumer_of_a_memoized_blank_invalidates_when_an_earlier_producer_grows() {
    let mut book = book(&[("A1", "Choice"), ("B2", "SUM(B1)"), ("B3", "B1+0")]);
    book.define_name("Choice", "IF(C1,{10,20},10)", None)
        .unwrap();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let context = EvaluationContext::default();
    {
        let mut e = overlay.evaluation(&context).unwrap();
        assert_eq!(e.cell(at("B2")).unwrap(), 0.0.into());
        assert_eq!(e.cell(at("B3")).unwrap(), 0.0.into());
    }
    overlay.set_value(at("C1"), true).unwrap();
    assert!(overlay.cached_value(at("B2")).is_none());
    assert!(overlay.cached_value(at("B3")).is_none());
    let mut e = overlay.evaluation(&context).unwrap();
    assert_eq!(e.cell(at("B2")).unwrap(), 20.0.into());
    assert_eq!(e.cell(at("B3")).unwrap(), 20.0.into());
}
#[test]
fn a_later_failing_sheet_preserves_previously_successful_values_and_changes() {
    let mut book = book(&[("A1", "{1,2}")]);
    book.add_sheet("Other")
        .unwrap()
        .set_formula("A1".parse().unwrap(), "{1,2}+B1")
        .unwrap();
    let other = EvaluationTarget {
        sheet: 1,
        address: "B1".parse().unwrap(),
    };
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(direct.cell(at("B1")).unwrap(), 2.0.into());
    let before = direct.changes();
    assert!(direct.cell(other).is_err());
    assert_eq!(direct.changes(), before);
    assert!(
        direct
            .values()
            .any(|(target, value)| target == at("B1") && *value == CellValue::Number(2.))
    );
    assert!(direct.values().all(|(target, _)| target.sheet == 0));
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    let mut e = overlay.evaluation(&context).unwrap();
    assert_eq!(e.cell(at("B1")).unwrap(), 2.0.into());
    let before = e.result_changes().unwrap();
    assert!(e.cell(other).is_err());
    assert_eq!(e.result_changes().unwrap(), before);
    assert!(e.values().all(|(target, _)| target.sheet == 0));
}
#[test]
fn real_producer_dependencies_still_obey_depth_budgets_atomically() {
    let mut book = book(&[]);
    for row in (1..=21).step_by(2) {
        let formula = if row == 21 {
            "{1,2}".to_owned()
        } else {
            format!("{{1,2}}+A{}", row + 2)
        };
        book.sheet_mut(0)
            .unwrap()
            .set_formula(format!("A{row}").parse().unwrap(), formula)
            .unwrap();
    }
    let mut context = EvaluationContext::default();
    context.limits = context.limits.max_depth(5);
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    assert!(
        matches!(direct.cell(at("B1")),Err(poirot::eval::EvaluationFailure::Budget(failure)) if failure.kind==poirot::eval::BudgetKind::Depth)
    );
    assert_eq!(direct.values().count(), 0);
    assert!(direct.changes().is_empty());
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let mut e = overlay.evaluation(&context).unwrap();
    assert!(e.cell(at("B1")).is_err());
    assert_eq!(e.values().count(), 0);
    assert!(e.result_changes().unwrap().is_empty());
}
#[test]
fn unresolved_resource_limited_producers_do_not_silently_turn_children_into_blanks() {
    use poirot::eval::{EvaluationFailure, overlay::EvaluationPolicy};
    let book = book(&[("A1", "{1,2}")]);
    let policy = EvaluationPolicy {
        parse: poirot_parsing::formula::FormulaParsePolicy {
            max_expressions: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy.clone())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let mut e = overlay.evaluation(&context).unwrap();
    assert!(matches!(
        e.cell(at("B1")),
        Err(EvaluationFailure::ResourceLimit { .. })
    ));
    assert_eq!(e.values().count(), 0);
    assert!(e.result_changes().unwrap().is_empty());
    let mut direct = Evaluation::with_workbook(&book)
        .with_policy(policy)
        .build(Default::default())
        .unwrap();
    assert!(matches!(
        direct.cell(at("B1")),
        Err(EvaluationFailure::ResourceLimit { .. })
    ));
    let malformed = book_with_syntax_error();
    let mut direct = Evaluation::with_workbook(&malformed)
        .build(Default::default())
        .unwrap();
    assert_eq!(direct.cell(at("B1")).unwrap(), CellValue::Blank);
    assert_eq!(
        direct.cell(at("A1")).unwrap(),
        CellValue::Error(CellError::Value)
    );
}
fn book_with_syntax_error() -> Workbook {
    book(&[("A1", "1+")])
}
#[test]
fn extension_created_arrays_publish_their_actual_growing_and_shrinking_dimensions() {
    use poirot::eval::{extensions::*, registry::Arity};
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "GROW",
            "original-spill-test-v1",
            ExtensionContract {
                arity: Arity {
                    minimum: 1,
                    maximum: 1,
                },
                evaluation: ExtensionEvaluationStrategy::Eager,
                capabilities: ExtensionCapabilities {
                    arrays: true,
                    references: true,
                    context: false,
                },
                cacheability: ExtensionCacheability::Deterministic,
            },
            |host: &mut dyn ExtensionHost| {
                let growing = host.scalar(0)?.value == CellValue::Boolean(true);
                Ok(ExtensionOutput::Array(if growing {
                    vec![vec![1.0.into(), 2.0.into()], vec![3.0.into(), 4.0.into()]]
                } else {
                    vec![vec![9.0.into()]]
                }))
            },
        )
        .unwrap();
    let book = book(&[("A1", "GROW(F1)"), ("D1", "SUM(B1:B2)")]);
    let mut overlay = Overlay::with_workbook(&book)
        .with_extensions(&registry)
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    for (growing, expected) in [(false, 0.0), (true, 6.0), (false, 0.0)] {
        overlay.set_value(at("F1"), growing).unwrap();
        let mut e = overlay.evaluation(&context).unwrap();
        assert_eq!(e.cell(at("D1")).unwrap(), expected.into());
        assert_eq!(
            e.cell(at("B2")).unwrap(),
            if growing {
                4.0.into()
            } else {
                CellValue::Blank
            }
        );
    }
}
#[test]
fn parser_policy_changes_reclassify_formerly_proven_scalar_sources() {
    use poirot::eval::{EvaluationFailure, overlay::EvaluationPolicy};
    let book = book(&[("A1", "A2")]);
    let context = EvaluationContext::default();
    let policy = EvaluationPolicy {
        parse: poirot_parsing::formula::FormulaParsePolicy {
            max_rows: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C1"))
            .unwrap(),
        CellValue::Blank
    );
    overlay.set_policy(policy.clone()).unwrap();
    assert!(matches!(
        overlay.evaluation(&context).unwrap().cell(at("C1")),
        Err(EvaluationFailure::ResourceLimit { .. })
    ));
    let mut direct = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert_eq!(direct.cell(at("C1")).unwrap(), CellValue::Blank);
    direct.set_policy(policy).unwrap();
    assert!(matches!(
        direct.cell(at("C1")),
        Err(EvaluationFailure::ResourceLimit { .. })
    ));
}
#[test]
fn cross_sheet_nonowners_do_not_create_an_artificial_producer_cycle() {
    let mut book = book(&[("A1", "{1,2}+SUM(Other!A6)")]);
    book.add_sheet("Other")
        .unwrap()
        .set_formula("D1".parse().unwrap(), "{1,2}+S!B1")
        .unwrap();
    let first = at("B1");
    let second = EvaluationTarget {
        sheet: 1,
        address: "E1".parse().unwrap(),
    };
    for targets in [[first, second], [second, first]] {
        let mut direct = Evaluation::with_workbook(&book)
            .build(Default::default())
            .unwrap();
        let mut overlay = Overlay::with_workbook(&book).build().unwrap();
        let context = EvaluationContext::default();
        for _ in 0..2 {
            let mut e = overlay.evaluation(&context).unwrap();
            for target in targets {
                let expected = if target.sheet == 0 { 2.0 } else { 4.0 };
                assert_eq!(direct.cell(target).unwrap(), expected.into());
                assert_eq!(e.cell(target).unwrap(), expected.into());
            }
            // Both touched sheets are complete at the first public boundary.
            assert!(
                e.values()
                    .any(|(target, value)| target == first && *value == CellValue::Number(2.))
            );
            assert!(
                e.values()
                    .any(|(target, value)| target == second && *value == CellValue::Number(4.))
            );
        }
    }
}
#[test]
fn finalized_children_in_consumed_ranges_are_not_unpublished_self_reads() {
    let mut registry = poirot::eval::extensions::ExtensionRegistry::new();
    use poirot::eval::{extensions::*, registry::Arity};
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "BLANKS",
            "original-blank-spill-v1",
            ExtensionContract {
                arity: Arity {
                    minimum: 0,
                    maximum: 0,
                },
                evaluation: ExtensionEvaluationStrategy::Eager,
                capabilities: ExtensionCapabilities {
                    arrays: true,
                    references: false,
                    context: false,
                },
                cacheability: ExtensionCacheability::Deterministic,
            },
            |_: &mut dyn ExtensionHost| {
                Ok(ExtensionOutput::Array(vec![vec![
                    CellValue::Blank,
                    CellValue::Blank,
                    CellValue::Blank,
                    CellValue::Blank,
                ]]))
            },
        )
        .unwrap();
    for read in ["D5", "SUM(D5:D5)"] {
        let book = book(&[
            ("D1", &format!("{{1;2;3;4;5}}+{read}+F1")),
            ("A5", "BLANKS()"),
        ]);
        let mut overlay = Overlay::with_workbook(&book)
            .with_extensions(&registry)
            .build()
            .unwrap();
        let context = EvaluationContext::default();
        let mut e = overlay.evaluation(&context).unwrap();
        // Strict priority would make D5 part of D1's own footprint. The
        // equation then has no stable value; a supplier-first answer is invalid.
        assert!(e.cell(at("D1")).is_err());
        assert_eq!(e.values().count(), 0);
        assert!(e.result_changes().unwrap().is_empty());
    }
}
#[test]
fn retained_disjoint_supplier_restores_its_complete_footprint_before_range_dependency_reads() {
    let book = book(&[("D1", "{1,2}+SUM(A6)+F1"), ("A5", "{10;20}")]);
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("E1"))
            .unwrap(),
        22.0.into()
    );
    overlay.set_value(at("F1"), 1.0).unwrap();
    assert!(overlay.cached_value(at("A5")).is_some());
    assert!(overlay.cached_value(at("D1")).is_none());
    let mut warm = overlay.evaluation(&context).unwrap();
    assert_eq!(warm.cell(at("E1")).unwrap(), 23.0.into());
    assert_eq!(warm.cell(at("A5")).unwrap(), 10.0.into());
    assert_eq!(warm.cell(at("A6")).unwrap(), 20.0.into());
    assert_eq!(warm.usage().formula_executions, 1);
}

#[test]
fn saved_ownership_describes_file_state_while_evaluation_uses_current_geometry() {
    let mut workbook = book(&[("A1", "SEQUENCE(2)")]);
    let sheet = workbook.sheet_mut(0).unwrap();
    let anchor = at("A1").address;
    sheet
        .set_spill_extent(anchor, at("A3").address, "SEQUENCE(2)")
        .unwrap();
    sheet
        .set_spill_cached_value(at("A2").address, anchor, "SEQUENCE(2)", 99.0.into())
        .unwrap();
    sheet
        .set_spill_cached_value(at("A3").address, anchor, "SEQUENCE(2)", 100.0.into())
        .unwrap();
    assert_eq!(sheet.spill_owner(anchor), None);
    assert_eq!(sheet.spill_owner(at("A3").address), Some(anchor));
    // SEQUENCE(2) produces exactly two rows. Saved geometry is not evidence that
    // the third coordinate belongs to the currently evaluated result.
    verify(&workbook, &[("A2", 2.0.into()), ("A3", CellValue::Blank)]);
    let sheet = workbook.sheet(0).unwrap();
    assert_eq!(sheet.spill_extent(anchor), Some(at("A3").address));
    assert_eq!(sheet.spill_owner(at("A3").address), Some(anchor));
}
