//! Typed clocks stay exact through execution, provider contexts, and restored caches.
use poirot::eval::EvaluationContext;
use poirot::eval::EvaluationTarget;
use poirot::eval::functions::extensions::*;
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::model::{CellValue, Workbook};
use std::time::{Duration, UNIX_EPOCH};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

#[test]
fn direct_overlay_and_restored_date_results_support_fractional_pre_epoch_time() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula(at("A1").address, "NOW()").unwrap();
    sheet.set_formula(at("B1").address, "TODAY()").unwrap();
    let context = EvaluationContext {
        as_of_time: UNIX_EPOCH - Duration::from_millis(500),
        ..Default::default()
    };
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let expected = direct.outputs(&[at("A1"), at("B1")]).unwrap();
    assert!(
        matches!(expected[0].value,CellValue::Number(value) if (value-(25569. - 0.5/86400.)).abs()<1e-10)
    );
    assert_eq!(expected[1].value, CellValue::Number(25568.));
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .outputs(&[at("A1"), at("B1")])
            .unwrap(),
        expected
    );
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (baseline, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach(&baseline).unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .outputs(&[at("A1"), at("B1")])
            .unwrap(),
        expected
    );
    direct
        .begin(EvaluationContext {
            as_of_time: context.as_of_time + Duration::from_secs(86400),
            ..context
        })
        .unwrap();
    assert_eq!(direct.cell(at("B1")).unwrap(), CellValue::Number(25569.));
}

#[test]
fn extension_and_retained_context_identity_preserve_single_nanosecond_changes() {
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "NANOS",
            "nanos/v1",
            ExtensionContract {
                arity: poirot::eval::functions::registry::Arity {
                    minimum: 0,
                    maximum: 0,
                },
                evaluation: ExtensionEvaluationStrategy::Eager,
                capabilities: ExtensionCapabilities {
                    references: false,
                    arrays: false,
                    context: true,
                },
                cacheability: ExtensionCacheability::Deterministic,
            },
            |host: &mut dyn ExtensionHost| {
                let time = host.context()?.as_of_time;
                let nanos = time.duration_since(UNIX_EPOCH).unwrap().subsec_nanos();
                Ok(ExtensionOutput::Scalar(CellValue::Number(f64::from(nanos))))
            },
        )
        .unwrap();
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula(at("A1").address, "NANOS()").unwrap();
    sheet.set_formula(at("B1").address, "A1+10").unwrap();
    let first = EvaluationContext {
        as_of_time: UNIX_EPOCH + Duration::new(1_700_000_000, 1),
        ..Default::default()
    };
    let second = EvaluationContext {
        as_of_time: first.as_of_time + Duration::from_nanos(1),
        ..first.clone()
    };
    let mut direct = Evaluation::with_workbook(&book)
        .with_extensions(&registry)
        .build(first.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_extensions(&registry)
        .build()
        .unwrap();
    assert_eq!(direct.cell(at("B1")).unwrap(), CellValue::Number(11.));
    assert_eq!(
        overlay.evaluation(&first).unwrap().cell(at("B1")).unwrap(),
        CellValue::Number(11.)
    );
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (baseline, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach_with_extensions(&baseline, &registry).unwrap();
    let mut unchanged = restored.evaluation(&first).unwrap();
    assert_eq!(unchanged.cell(at("B1")).unwrap(), CellValue::Number(11.));
    assert_eq!(unchanged.usage().formula_executions, 0);
    drop(unchanged);
    direct.begin(second.clone()).unwrap();
    assert_eq!(direct.cell(at("B1")).unwrap(), CellValue::Number(12.));
    for workspace in [&mut overlay, &mut restored] {
        let mut changed = workspace.evaluation(&second).unwrap();
        assert_eq!(changed.cell(at("B1")).unwrap(), CellValue::Number(12.));
        assert_eq!(changed.usage().formula_executions, 2);
    }
}
