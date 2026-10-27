use super::super::incremental::Overlay;
use super::super::snapshot_state::SnapshotState;
use crate::eval::config::{EvaluationContext, EvaluationPolicy};
use crate::eval::coordinates::EvaluationTarget;
use crate::model::{CellAddress, CellValue, Workbook};
use std::borrow::Borrow;

/// Evaluate one output and preserve both its arithmetic result and measured source work.
fn evaluated<W: Borrow<Workbook>>(
    overlay: &mut Overlay<'_, W>,
    context: &EvaluationContext,
    target: EvaluationTarget,
) -> (
    CellValue,
    crate::eval::limits::EvaluationUsage,
    crate::eval::limits::RunUsage,
) {
    let mut cycle = overlay.calculation_with_profiling(context, true).unwrap();
    let values = cycle.outputs(&[target]).unwrap();
    let report = cycle.finish();
    report.result.unwrap();
    (
        values[0].value.clone(),
        report.usage.unwrap(),
        report.execution.unwrap(),
    )
}

#[test]
fn first_changed_cycle_reuses_restored_source_discovery() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1".parse().unwrap(), 1.0).unwrap();
    for row in 1..=512 {
        sheet
            .set_formula(format!("B{row}").parse().unwrap(), "A1*2")
            .unwrap();
    }
    sheet
        .set_formula("C512".parse().unwrap(), "B512+A512+1")
        .unwrap();
    let mut retained = Overlay::with_workbook(&book).build().unwrap();
    let output = retained.target("S!C512").unwrap();
    let input = retained.target("S!A1").unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        evaluated(&mut retained, &context, output).0,
        CellValue::Number(3.0)
    );
    let coverage =
        crate::eval::spill::inspected_prefixes(&retained.source.spill_candidates.borrow());
    assert!(!coverage.is_empty());
    let bytes =
        crate::eval::artifact::encode_workspace(retained.snapshot(), Default::default()).unwrap();
    let artifact = crate::eval::artifact::decode(
        &bytes,
        Default::default(),
        crate::eval::artifact::PreparedPolicy::RequireCompatible,
    )
    .unwrap();
    let crate::eval::artifact::RestoredCheckpoint::Prepared(mut restored) =
        artifact.into_checkpoint().unwrap()
    else {
        panic!("prepared snapshot")
    };
    assert_eq!(
        crate::eval::spill::inspected_prefixes(&restored.source.spill_candidates.borrow()),
        coverage
    );
    assert_eq!(restored.cached_cells(), retained.cached_cells());
    retained.set_value(input, 2.0).unwrap();
    restored.set_value(input, 2.0).unwrap();
    let a = evaluated(&mut retained, &context, output);
    let b = evaluated(&mut restored, &context, output);
    assert_eq!(a.0, CellValue::Number(5.0));
    assert_eq!(a.0, b.0);
    assert_eq!(a.1.sparse_entries, b.1.sparse_entries);
    assert_eq!(a.2.formula_executions, b.2.formula_executions);
    assert!(a.1.sparse_entries < 512);
    assert_eq!(
        evaluated(&mut restored, &context, output)
            .2
            .formula_executions,
        0
    );
}

#[test]
fn restored_lookup_prefixes_keep_first_match_and_invalidate_on_edits_and_clock_changes() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula("A1".parse().unwrap(), "TODAY()").unwrap();
    for (cell, value) in [
        ("A2", 2.0),
        ("A3", 2.0),
        ("B2", 20.0),
        ("B3", 30.0),
        ("C1", 2.0),
        ("E1", 0.0),
    ] {
        sheet.set_value(cell.parse().unwrap(), value).unwrap();
    }
    sheet
        .set_formula("A4".parse().unwrap(), "UNSUPPORTED()")
        .unwrap();
    sheet
        .set_formula("D1".parse().unwrap(), "VLOOKUP(C1,A1:B4,2,FALSE)+E1")
        .unwrap();
    let mut retained = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
            arrays: crate::eval::config::ArrayResultPolicy::ScalarIntersection,
            ..Default::default()
        })
        .build()
        .unwrap();
    let output = retained.target("S!D1").unwrap();
    let input = retained.target("S!E1").unwrap();
    let context = EvaluationContext {
        as_of_time: std::time::UNIX_EPOCH,
        ..Default::default()
    };
    let first = evaluated(&mut retained, &context, output);
    assert_eq!(first.0, CellValue::Number(20.0));
    assert_eq!(first.2.lookup_prefix_scans, 2);
    let mut missing_context = retained.snapshot().into_owned().unwrap();
    let SnapshotState::Owned(payload) = &mut missing_context.state else {
        panic!("owned snapshot")
    };
    payload.context = None;
    payload.cached.clear();
    assert!(
        missing_context.restore().is_err(),
        "lookup evidence requires its execution context"
    );
    let (book, state) = retained.snapshot().restore().unwrap();
    let mut restored = state.attach(book).unwrap();
    for step in 1..=3 {
        retained.set_value(input, f64::from(step)).unwrap();
        restored.set_value(input, f64::from(step)).unwrap();
        let current = if step == 2 {
            EvaluationContext {
                as_of_time: std::time::UNIX_EPOCH + std::time::Duration::from_secs(86_400),
                ..context.clone()
            }
        } else {
            context.clone()
        };
        let a = evaluated(&mut retained, &current, output);
        let b = evaluated(&mut restored, &current, output);
        assert_eq!(a.0, CellValue::Number(20.0 + f64::from(step)));
        assert_eq!(a.0, b.0);
        assert_eq!(a.2.lookup_prefix_hits, b.2.lookup_prefix_hits);
        assert_eq!(a.2.lookup_prefix_scans, b.2.lookup_prefix_scans);
        if step == 1 {
            assert_eq!(a.2.lookup_prefix_scans, 0);
            assert_eq!(a.2.lookup_prefix_hits, 1);
        } else {
            assert_eq!(a.2.lookup_prefix_scans, 2);
        }
    }
    let changed = retained.target("S!A2").unwrap();
    retained.set_value(changed, 9.0).unwrap();
    restored.set_value(changed, 9.0).unwrap();
    let a = evaluated(&mut retained, &context, output);
    let b = evaluated(&mut restored, &context, output);
    assert_eq!(a.0, CellValue::Number(33.0));
    assert_eq!(a.0, b.0);
    assert_eq!(a.2.lookup_prefix_scans, 3);
    assert_eq!(a.2.lookup_prefix_scans, b.2.lookup_prefix_scans);
}

#[test]
fn restored_discovery_expires_when_spill_source_changes() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_formula("A1".parse().unwrap(), "SEQUENCE(2,1,1,10)")
        .unwrap();
    sheet.set_formula("D1".parse().unwrap(), "A2*2").unwrap();
    let mut retained = Overlay::with_workbook(&book).build().unwrap();
    let output = retained.target("S!D1").unwrap();
    let producer = retained.target("S!A1").unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        evaluated(&mut retained, &context, output).0,
        CellValue::Number(22.0)
    );
    let (book, state) = retained.snapshot().restore().unwrap();
    let mut restored = state.attach(book).unwrap();
    // SEQUENCE starts at 3 or 1 with step 10; the child is 13 or 11.
    // A one-row sequence removes the child, whose arithmetic blank is zero.
    for (formula, expected) in [
        ("SEQUENCE(2,1,3,10)", 26.0),
        ("SEQUENCE(1,1,3,10)", 0.0),
        ("SEQUENCE(2,1,1,10)", 22.0),
    ] {
        retained.set_formula(producer, formula).unwrap();
        restored.set_formula(producer, formula).unwrap();
        let a = evaluated(&mut retained, &context, output);
        let b = evaluated(&mut restored, &context, output);
        assert_eq!(a.0, CellValue::Number(expected));
        assert_eq!(a.0, b.0);
        assert_eq!(a.1.sparse_entries, b.1.sparse_entries);
        assert_eq!(a.2.formula_executions, b.2.formula_executions);
    }
}

#[test]
fn malformed_discovery_prefixes_are_rejected_before_attachment() {
    for prefix in [
        (1, CellAddress { row: 0, col: 0 }),
        (
            0,
            CellAddress {
                row: u32::MAX,
                col: 0,
            },
        ),
    ] {
        let mut book = Workbook::new();
        book.add_sheet("S").unwrap();
        let overlay = Overlay::with_workbook(&book).build().unwrap();
        let mut snapshot = overlay.snapshot().into_owned().unwrap();
        let SnapshotState::Owned(payload) = &mut snapshot.state else {
            panic!("owned snapshot")
        };
        payload.inspected_prefixes.push(prefix);
        assert!(snapshot.restore().is_err());
    }
}

#[test]
fn duplicate_discovery_prefixes_are_rejected() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    let overlay = Overlay::with_workbook(&book).build().unwrap();
    let mut snapshot = overlay.snapshot().into_owned().unwrap();
    let SnapshotState::Owned(payload) = &mut snapshot.state else {
        panic!("owned snapshot")
    };
    payload.inspected_prefixes = vec![(0, CellAddress { row: 1, col: 1 }); 2];
    assert!(snapshot.restore().is_err());
}

#[test]
fn source_only_preparation_does_not_invent_discovery_or_results() {
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula("A1".parse().unwrap(), "1+2")
        .unwrap();
    let overlay = Overlay::with_workbook(&book).build().unwrap();
    let (book, state) = overlay.snapshot().restore().unwrap();
    let mut restored = state.attach(book).unwrap();
    assert!(restored.source.spill_candidates.borrow().is_empty());
    assert_eq!(restored.cached_cells(), 0);
}
