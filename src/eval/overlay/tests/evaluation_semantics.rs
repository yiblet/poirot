use crate::eval::compile::compiler::{FormulaCompiler, PreparationPolicy};
use crate::eval::config::EvaluationPolicy;
use crate::eval::config::{ArrayResultPolicy, EvaluationInputs, RunConfiguration};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::entry::{ExtensionExecution, run_cell_with_extensions};
use crate::eval::functions::extensions::*;
use crate::eval::limits::{EvaluationBudgets, EvaluationError};
use crate::eval::overlay;
use crate::eval::overlay::incremental::Overlay;
use crate::eval::run::{EvaluationRun, WorkbookView};
use crate::model::{CellError, CellValue, Workbook};
use std::time::{Duration, UNIX_EPOCH};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

fn workbook(formulas: &[(&str, &str)]) -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for &(address, formula) in formulas {
        sheet.set_formula(at(address).address, formula).unwrap();
    }
    book
}

fn collect(outputs: Vec<crate::eval::values::CellEvaluation>) -> BTreeMap<String, CellValue> {
    outputs
        .into_iter()
        .map(|v| (v.target.address.to_string(), v.value))
        .collect()
}

fn requests<W: std::borrow::Borrow<Workbook>>(
    overlay: &mut Overlay<'_, W>,
    context: &crate::eval::config::EvaluationContext,
    order: &[&str],
    batch: bool,
) -> BTreeMap<String, CellValue> {
    let mut evaluation = overlay.evaluation(context).unwrap();
    let targets = order.iter().map(|s| at(s)).collect::<Vec<_>>();
    if batch {
        collect(evaluation.outputs(&targets).unwrap())
    } else {
        targets
            .into_iter()
            .map(|t| (t.address.to_string(), evaluation.cell(t).unwrap()))
            .collect()
    }
}

#[test]
fn random_identity_survives_order_batching_cache_warmth_and_snapshot_restore() {
    let book = workbook(&[
        ("A1", "RAND()+RAND()"),
        ("B1", "RANDBETWEEN(1,1000000)"),
        ("C1", "A1+B1"),
        ("D1", "7*3"),
    ]);
    let inputs = EvaluationInputs {
        randomness_seed: 73,
        as_of_time: UNIX_EPOCH + Duration::from_secs(123456),
        ..Default::default()
    };
    let context = crate::eval::config::EvaluationContext::default().with_clock(
        inputs.as_of_time,
        inputs.randomness_seed,
        inputs.localization,
    );
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
    let mut direct = EvaluationRun::new(
        WorkbookView::new(&book, &plan, &policy).unwrap(),
        inputs,
        Default::default(),
    )
    .unwrap();
    let targets = [at("A1"), at("B1"), at("C1"), at("D1")];
    let expected = targets
        .iter()
        .map(|t| (t.address.to_string(), direct.evaluate_cell(*t).unwrap()))
        .collect::<BTreeMap<_, _>>();
    for order in [["A1", "B1", "C1", "D1"], ["D1", "C1", "B1", "A1"]] {
        for batch in [false, true] {
            let mut overlay = Overlay::with_workbook(&book).build().unwrap();
            for _ in 0..2 {
                assert_eq!(requests(&mut overlay, &context, &order, batch), expected);
            }
            let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
            let (restored_workbook, restored_state) =
                Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
            let mut restored = restored_state.attach(&restored_workbook).unwrap();
            assert_eq!(requests(&mut restored, &context, &order, batch), expected);
            // A new boundary retains results whose captured random identity is unchanged.
            let mut evaluation = restored.evaluation(&context).unwrap();
            evaluation.outputs(&targets).unwrap();
            assert_eq!(evaluation.usage().formula_executions, 0);
            assert_eq!(evaluation.usage().external_cache_hits, 4);
        }
    }
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let changed = crate::eval::config::EvaluationContext {
        randomness_seed: 74,
        ..context.clone()
    };
    assert_ne!(
        requests(&mut overlay, &changed, &["A1"], true)["A1"],
        expected["A1"]
    );
}

#[test]
fn source_ordered_spill_conflicts_match_direct_warm_and_restored_execution() {
    let book = workbook(&[("D2", "{1,2,3}"), ("E1", "{8;9;10}")]);
    let orderings = [
        ["D2", "E1", "E2", "E3", "F2"],
        ["E1", "D2", "F2", "E3", "E2"],
        ["E2", "F2", "E3", "D2", "E1"],
    ];
    let configuration =
        RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::DynamicSpill);
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
    let expected = BTreeMap::from([
        ("D2".to_owned(), CellValue::Error(CellError::Spill)),
        ("E1".to_owned(), CellValue::Number(8.)),
        ("E2".to_owned(), CellValue::Number(9.)),
        ("E3".to_owned(), CellValue::Number(10.)),
        ("F2".to_owned(), CellValue::Blank),
    ]);
    for order in orderings {
        let mut direct = EvaluationRun::new_with_configuration(
            WorkbookView::new(&book, &plan, &policy).unwrap(),
            &configuration,
            Default::default(),
        )
        .unwrap();
        assert_eq!(
            order
                .iter()
                .map(|s| (s.to_string(), direct.evaluate_cell(at(s)).unwrap()))
                .collect::<BTreeMap<_, _>>(),
            expected
        );
        for batch in [false, true] {
            let mut overlay = Overlay::with_workbook(&book)
                .with_policy(EvaluationPolicy {
                    arrays: ArrayResultPolicy::DynamicSpill,
                    ..Default::default()
                })
                .build()
                .unwrap();
            let context = crate::eval::config::EvaluationContext::default();
            assert_eq!(requests(&mut overlay, &context, &order, batch), expected);
            assert_eq!(requests(&mut overlay, &context, &order, batch), expected);
            let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
            let (restored_workbook, restored_state) =
                Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
            let mut restored = restored_state.attach(&restored_workbook).unwrap();
            assert_eq!(requests(&mut restored, &context, &order, batch), expected);
            // Removing the winner must invalidate a retained losing producer too.
            restored.remove_cell(at("E1")).unwrap();
            assert_eq!(
                requests(&mut restored, &context, &["F2", "D2"], batch)["D2"],
                CellValue::Number(1.)
            );
        }
    }
}

#[test]
fn cache_hits_keep_result_admission_limits_but_save_actual_work() {
    let sum = format!("SUM({})", vec!["1"; 100].join(","));
    let book = workbook(&[("A1", "REPT(\"x\",100)"), ("D1", "{1,2;3,4}"), ("G1", &sum)]);
    for (output, budgets, kind) in [
        (
            "A1",
            EvaluationBudgets {
                text_bytes: 50,
                ..Default::default()
            },
            crate::eval::limits::BudgetKind::TextBytes,
        ),
        (
            "D1",
            EvaluationBudgets {
                materialized_values: 3,
                ..Default::default()
            },
            crate::eval::limits::BudgetKind::Materialization,
        ),
    ] {
        let policy = EvaluationPolicy {
            arrays: ArrayResultPolicy::DynamicSpill,
            ..Default::default()
        };
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(policy.clone())
            .build()
            .unwrap();
        overlay
            .evaluation(&Default::default())
            .unwrap()
            .cell(at(output))
            .unwrap();
        let context = crate::eval::config::EvaluationContext {
            limits: budgets.into(),
            ..Default::default()
        };
        for mut workspace in [
            Overlay::with_workbook(&book)
                .with_policy(policy)
                .build()
                .unwrap(),
            overlay,
        ] {
            let failure = workspace
                .evaluation(&context)
                .unwrap()
                .cell(at(output))
                .unwrap_err();
            assert!(
                matches!(failure, EvaluationError::Budget(ref b) if b.kind == kind),
                "{failure:?}"
            );
        }
    }
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    requests(&mut overlay, &Default::default(), &["G1"], true);
    let context = crate::eval::config::EvaluationContext {
        limits: crate::eval::limits::EvaluationLimits::default().max_work(150),
        ..Default::default()
    };
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("G1"))
            .unwrap(),
        CellValue::Number(100.)
    );
    let mut fresh = Overlay::with_workbook(&book).build().unwrap();
    assert!(matches!(
        fresh.evaluation(&context).unwrap().cell(at("G1")),
        Err(EvaluationError::Budget(_))
    ));
}

fn provider_registry(
    cacheability: ExtensionCacheability,
    calls: Arc<AtomicUsize>,
) -> ExtensionRegistry {
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "CUSTOM",
            "code/v1",
            ExtensionContract {
                arity: crate::eval::functions::registry::Arity {
                    minimum: 1,
                    maximum: 1,
                },
                evaluation: ExtensionEvaluationStrategy::Deferred,
                capabilities: ExtensionCapabilities {
                    references: true,
                    arrays: false,
                    context: true,
                },
                cacheability,
            },
            move |host: &mut dyn ExtensionHost| {
                calls.fetch_add(1, Ordering::SeqCst);
                let CellValue::Number(value) = host.scalar(0)?.value else {
                    panic!("numeric input")
                };
                Ok(ExtensionOutput::Scalar(CellValue::Number(
                    value + host.context()?.randomness_seed as f64,
                )))
            },
        )
        .unwrap();
    registry
}

#[test]
fn extension_cacheability_tracks_inputs_context_and_snapshot_identity() {
    for cacheability in [
        ExtensionCacheability::Deterministic,
        ExtensionCacheability::Versioned {
            external_inputs_revision: 7,
        },
        ExtensionCacheability::Volatile,
        ExtensionCacheability::Opaque,
    ] {
        let calls = Arc::new(AtomicUsize::new(0));
        let registry = provider_registry(cacheability, Arc::clone(&calls));
        let mut book = workbook(&[("B1", "CUSTOM(A1)"), ("C1", "B1+1")]);
        book.sheet_mut(0)
            .unwrap()
            .set_value(at("A1").address, 3.)
            .unwrap();
        let direct_registry = provider_registry(cacheability, Arc::new(AtomicUsize::new(0)));
        let direct = run_cell_with_extensions(
            &book,
            0,
            at("C1").address,
            ExtensionExecution {
                registry: &direct_registry,
                context: EvaluationInputs {
                    randomness_seed: 10,
                    ..Default::default()
                },
                budgets: Default::default(),
            },
        );
        assert!(matches!(
            direct.outcome,
            crate::eval::limits::EvaluationOutcome::Complete {
                value: CellValue::Number(14.),
                ..
            }
        ));
        let mut overlay = Overlay::with_workbook(&book)
            .with_extensions(&registry)
            .build()
            .unwrap();
        let context = crate::eval::config::EvaluationContext {
            randomness_seed: 10,
            ..Default::default()
        };
        for _ in 0..2 {
            assert_eq!(
                requests(&mut overlay, &context, &["C1", "B1"], true)["C1"],
                CellValue::Number(14.)
            );
        }
        let reusable = matches!(
            cacheability,
            ExtensionCacheability::Deterministic | ExtensionCacheability::Versioned { .. }
        );
        assert_eq!(calls.load(Ordering::SeqCst), if reusable { 1 } else { 2 });
        let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
        let (restored_workbook, restored_state) =
            Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
        let mut restored = restored_state
            .attach_with_extensions(&restored_workbook, &registry)
            .unwrap();
        requests(&mut restored, &context, &["C1"], true);
        assert_eq!(calls.load(Ordering::SeqCst), if reusable { 1 } else { 3 });
        restored.set_value(at("A1"), 8.).unwrap();
        assert_eq!(
            requests(&mut restored, &context, &["C1"], true)["C1"],
            CellValue::Number(19.)
        );
        let changed = crate::eval::config::EvaluationContext {
            randomness_seed: 20,
            ..context
        };
        assert_eq!(
            requests(&mut restored, &changed, &["C1"], true)["C1"],
            CellValue::Number(29.)
        );
        let other = provider_registry(
            ExtensionCacheability::Versioned {
                external_inputs_revision: 8,
            },
            Arc::clone(&calls),
        );
        assert!(matches!(
            Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap())
                .and_then(|(workbook, state)| state.attach_with_extensions(workbook, &other)),
            Err(overlay::OverlayError::RegistryMismatch)
        ));
    }
}

#[test]
fn editable_direct_extensions_clear_on_edits_and_refresh_at_explicit_boundaries() {
    for cacheability in [
        ExtensionCacheability::Deterministic,
        ExtensionCacheability::Versioned {
            external_inputs_revision: 7,
        },
        ExtensionCacheability::Volatile,
        ExtensionCacheability::Opaque,
    ] {
        let calls = Arc::new(AtomicUsize::new(0));
        let registry = provider_registry(cacheability, Arc::clone(&calls));
        let mut book = workbook(&[("B1", "CUSTOM(A1)"), ("C1", "B1+1")]);
        book.sheet_mut(0)
            .unwrap()
            .set_value(at("A1").address, 3.)
            .unwrap();
        let context = crate::eval::config::EvaluationContext {
            randomness_seed: 10,
            ..Default::default()
        };
        let mut direct = overlay::Evaluation::with_workbook(&book)
            .with_extensions(&registry)
            .build(context.clone())
            .unwrap();
        assert_eq!(direct.cell(at("C1")).unwrap(), CellValue::Number(14.));
        assert_eq!(direct.cell(at("C1")).unwrap(), CellValue::Number(14.));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        direct.set_value(at("A1"), 8.).unwrap();
        assert_eq!(direct.cell(at("C1")).unwrap(), CellValue::Number(19.));
        let bytes = postcard::to_allocvec(&direct.snapshot()).unwrap();
        let (baseline, state) =
            overlay::Evaluation::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
        let mut restored = state
            .into_evaluation_with_extensions(&baseline, context.clone(), &registry)
            .unwrap();
        assert_eq!(restored.cell(at("C1")).unwrap(), CellValue::Number(19.));
        assert_eq!(restored.compiled_usage().parse_attempts, 0);
        direct
            .begin(crate::eval::config::EvaluationContext {
                randomness_seed: 20,
                ..context
            })
            .unwrap();
        assert_eq!(direct.cell(at("C1")).unwrap(), CellValue::Number(29.));
        std::thread::scope(|scope| {
            scope.spawn(move || assert_eq!(direct.cell(at("C1")).unwrap(), CellValue::Number(29.)));
            scope.spawn(move || {
                assert_eq!(restored.cell(at("C1")).unwrap(), CellValue::Number(19.))
            });
        });
    }
}

#[test]
fn shared_providers_move_with_used_workspaces_and_snapshot_attachments() {
    let registry = Arc::new(provider_registry(
        ExtensionCacheability::Deterministic,
        Arc::new(AtomicUsize::new(0)),
    ));
    let mut book = workbook(&[("B1", "CUSTOM(A1)")]);
    book.sheet_mut(0)
        .unwrap()
        .set_value(at("A1").address, 3.)
        .unwrap();
    let book = Arc::new(book);
    let context = crate::eval::config::EvaluationContext {
        randomness_seed: 10,
        ..Default::default()
    };
    let mut workspace = Overlay::with_workbook(Arc::clone(&book))
        .with_shared_extensions(Arc::clone(&registry))
        .build()
        .unwrap();
    workspace
        .evaluation(&context)
        .unwrap()
        .cell(at("B1"))
        .unwrap();
    let mut direct = overlay::Evaluation::with_workbook(Arc::clone(&book))
        .with_shared_extensions(Arc::clone(&registry))
        .build(context.clone())
        .unwrap();
    direct.cell(at("B1")).unwrap();
    let bytes = postcard::to_allocvec(&workspace.snapshot()).unwrap();
    let (baseline, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state
        .attach_with_shared_extensions(Arc::new(baseline), registry)
        .unwrap();
    std::thread::spawn(move || {
        assert_eq!(
            workspace
                .evaluation(&context)
                .unwrap()
                .cell(at("B1"))
                .unwrap(),
            CellValue::Number(13.)
        );
        assert_eq!(
            restored
                .evaluation(&context)
                .unwrap()
                .cell(at("B1"))
                .unwrap(),
            CellValue::Number(13.)
        );
        direct.set_value(at("A1"), 8.).unwrap();
        assert_eq!(direct.cell(at("B1")).unwrap(), CellValue::Number(18.));
    })
    .join()
    .unwrap();
}
