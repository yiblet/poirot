use super::super::self_read::SelfRead;
use super::super::spill_preparation::SpillPreparation;
use crate::eval::config::ArrayResultPolicy;
use crate::eval::limits::{BudgetMeter, EvaluationBudgets, EvaluationError};
use crate::eval::reuse::SpillRole;
use crate::eval::session::{CellEvaluator, Session};
use crate::eval::values::{Area, AreaKind};
use crate::model::CellValue::{Error, Number};
use crate::model::{CellAddress, CellError, Workbook};

#[test]
fn publication_requires_successful_settlement_and_acknowledges_without_reallocating() {
    let key = (0, CellAddress { row: 0, col: 0 });
    let mut preparation = SpillPreparation::default();
    preparation
        .roles
        .insert(key, SpillRole::Anchor { last: key.1 });
    preparation.journal.push(key);
    let capacity = preparation.journal.capacity();
    preparation.settlement.begin(Default::default(), 0, 0);
    assert!(preparation.publication().is_err());
    preparation.settlement.finish();
    let publication = preparation.publication().unwrap();
    assert_eq!(publication.pending_cells(), &[key]);
    assert_eq!(publication.extent(key), Some(key.1));
    assert_eq!(publication.role(key).owner(key.1), Some(key.1));
    assert_eq!(publication.generated_count(), 1);
    assert!(publication.children().next().is_none());
    // Merely borrowing a batch, including an abandoned one, cannot consume it.
    assert_eq!(preparation.publication().unwrap().pending_cells(), &[key]);
    preparation.acknowledge_publication();
    assert!(
        preparation
            .publication()
            .unwrap()
            .pending_cells()
            .is_empty()
    );
    assert_eq!(preparation.journal.capacity(), capacity);
    preparation.failure = Some(EvaluationError::StalePlan);
    assert!(preparation.publication().is_err());
    assert!(matches!(preparation.admitted_role(key), SpillRole::Anchor { last } if last == key.1));
}

#[test]
fn evaluated_roles_distinguish_one_cell_error_blocked_and_child_results() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (address, formula) in [
        ("A1", "{1}"),
        ("C1", "{#N/A,2}"),
        ("F1", "{3,4}"),
        ("I1", "{5,6}"),
    ] {
        sheet.set_formula(address, formula).unwrap();
    }
    sheet.set_value("J1", 99.).unwrap();
    let mut session = Session::with_context(&book, Default::default());
    session.policies.array_result_policy = ArrayResultPolicy::DynamicSpill;
    for (address, expected, extent) in [
        ("A1", Number(1.), Some("A1")),
        ("C1", Error(CellError::NA), Some("D1")),
        ("G1", Number(4.), None),
        ("I1", Error(CellError::Spill), None),
    ] {
        let at: CellAddress = address.parse().unwrap();
        assert_eq!(
            CellEvaluator::new(&mut session).cell(0, at).unwrap(),
            expected
        );
        let publication = session.state.runtime.preparation.publication().unwrap();
        assert_eq!(
            publication.extent((0, at)),
            extent.map(|s| s.parse().unwrap())
        );
    }
    let child: CellAddress = "G1".parse().unwrap();
    let anchor: CellAddress = "F1".parse().unwrap();
    assert!(
        matches!(session.state.runtime.preparation.admitted_role((0, child)), SpillRole::Child { anchor: owner } if owner == anchor)
    );
    // Evaluation does not mutate saved worksheet ownership or geometry.
    assert_eq!(book.sheet(0).unwrap().spill_owner(child), None);
    assert_eq!(book.sheet(0).unwrap().spill_extent(anchor), None);
}

#[test]
fn producer_observations_share_lifecycle_and_keep_sparse_diagnostic_priority() {
    let key = (0, CellAddress { row: 0, col: 0 });
    let point = (0, CellAddress { row: 1, col: 1 });
    let other = (1, point.1);
    let area = Area {
        kind: AreaKind::Area,
        sheet: 0,
        r0: 1,
        r1: 1,
        c0: 1,
        c1: 1,
    };
    let mut preparation = SpillPreparation::default();
    let mut budget = BudgetMeter::new(Default::default()).unwrap();
    preparation
        .producer_reads
        .record_blank(key, point, &mut budget)
        .unwrap();
    preparation
        .producer_reads
        .record_blank(key, point, &mut budget)
        .unwrap();
    assert_eq!(preparation.producer_reads.blank_points[&key].len(), 1);
    assert_eq!(
        preparation.producer_reads.self_read(key, point),
        Some(SelfRead::Blank)
    );
    assert_eq!(preparation.producer_reads.self_read(key, other), None);
    preparation
        .producer_reads
        .record_sparse(key, area, &mut budget)
        .unwrap();
    assert_eq!(
        preparation.producer_reads.self_read(key, point),
        Some(SelfRead::Sparse)
    );
    preparation.producer_reads.forget(key);
    assert_eq!(preparation.producer_reads.self_read(key, point), None);
    preparation
        .producer_reads
        .record_sparse(key, area, &mut budget)
        .unwrap();
    preparation.producer_reads.clear();
    assert!(preparation.producer_reads.blank_points.is_empty());
    assert!(preparation.producer_reads.sparse_ranges.is_empty());
}

#[test]
fn preparation_reset_clears_both_observation_channels_and_reuses_their_buffers() {
    let key = (0, CellAddress { row: 0, col: 0 });
    let area = Area {
        kind: AreaKind::Area,
        sheet: 0,
        r0: 0,
        r1: 0,
        c0: 0,
        c1: 0,
    };
    let mut preparation = SpillPreparation::default();
    let mut budget = BudgetMeter::new(Default::default()).unwrap();
    preparation
        .producer_reads
        .record_blank(key, key, &mut budget)
        .unwrap();
    preparation
        .producer_reads
        .record_sparse(key, area, &mut budget)
        .unwrap();
    let capacities = (
        preparation.producer_reads.blank_points.capacity(),
        preparation.producer_reads.sparse_ranges.capacity(),
    );
    preparation.failure = Some(EvaluationError::StalePlan);
    preparation
        .roles
        .insert(key, SpillRole::Anchor { last: key.1 });
    preparation.reset();
    assert!(
        preparation
            .publication()
            .unwrap()
            .pending_cells()
            .is_empty()
    );
    assert!(matches!(preparation.admitted_role(key), SpillRole::None));
    assert!(preparation.producer_reads.blank_points.is_empty());
    assert!(preparation.producer_reads.sparse_ranges.is_empty());
    assert_eq!(
        (
            preparation.producer_reads.blank_points.capacity(),
            preparation.producer_reads.sparse_ranges.capacity()
        ),
        capacities
    );
}

#[test]
fn refused_self_read_recording_never_inserts_observations() {
    let key = (0, CellAddress { row: 0, col: 0 });
    let area = Area {
        kind: AreaKind::Area,
        sheet: 0,
        r0: 0,
        r1: 0,
        c0: 0,
        c1: 0,
    };
    for limits in [
        EvaluationBudgets {
            run_work: 0,
            ..Default::default()
        },
        EvaluationBudgets {
            materialized_values: 0,
            ..Default::default()
        },
    ] {
        let mut preparation = SpillPreparation::default();
        let mut budget = BudgetMeter::new(limits).unwrap();
        assert!(
            preparation
                .producer_reads
                .record_blank(key, key, &mut budget)
                .is_err()
        );
        assert!(
            preparation
                .producer_reads
                .record_sparse(key, area, &mut budget)
                .is_err()
        );
        assert_eq!(preparation.producer_reads.self_read(key, key), None);
    }
}
