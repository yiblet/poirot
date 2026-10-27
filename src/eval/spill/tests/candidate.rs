use super::super::producer_candidate::ProducerCandidate;
use super::super::spill_access::SpillAccess;
use crate::eval::config::EvaluationInputs;
use crate::eval::session::Session;
use crate::eval::values::{Area, AreaKind};
use crate::model::{CellAddress, ShapeBounds, Workbook};

#[test]
fn bounded_candidates_match_an_independent_coordinate_membership_oracle() {
    for sheet in 0..2 {
        for row in [0, 2, crate::model::MAX_ROWS - 1] {
            for col in [0, 3, crate::model::MAX_COLUMNS - 1] {
                let anchor = CellAddress { row, col };
                for shape in [
                    ShapeBounds::new(2, 4),
                    ShapeBounds::new(0, 1),
                    ShapeBounds::new(1, 0),
                    ShapeBounds::UNKNOWN,
                ] {
                    let candidate = ProducerCandidate::new(sheet, anchor, shape);
                    for target_sheet in 0..2 {
                        for r0 in [0, 1, 2, 4, crate::model::MAX_ROWS - 1] {
                            for c0 in [0, 1, 3, 5, crate::model::MAX_COLUMNS - 1] {
                                let demand = Area {
                                    kind: AreaKind::Area,
                                    sheet: target_sheet,
                                    r0,
                                    r1: r0.saturating_add(1).min(crate::model::MAX_ROWS - 1),
                                    c0,
                                    c1: c0.saturating_add(1).min(crate::model::MAX_COLUMNS - 1),
                                };
                                let row_inside = |r| {
                                    r >= row
                                        && shape.rows.is_none_or(|n| {
                                            u64::from(r) - u64::from(row) < u64::from(n.get())
                                        })
                                };
                                let col_inside = |c| {
                                    c >= col
                                        && shape.columns.is_none_or(|n| {
                                            u64::from(c) - u64::from(col) < u64::from(n.get())
                                        })
                                };
                                let oracle = sheet == target_sheet
                                    && (demand.r0..=demand.r1).any(row_inside)
                                    && (demand.c0..=demand.c1).any(col_inside);
                                assert_eq!(candidate.intersects(demand), oracle);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn source_owned_index_omits_scalar_and_preserves_static_disjointness() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (address, formula) in [
        ("A1", "SEQUENCE(3)"),
        ("C1", "SEQUENCE(A10,2)"),
        ("F1", "SUM(SEQUENCE(4))"),
        ("H1", "bad_name"),
        ("J1", "{1,2}"),
    ] {
        sheet.set_formula(address, formula).unwrap();
    }
    book.add_sheet("Other").unwrap();
    let mut session = Session::with_context(&book, EvaluationInputs::default());
    let candidates = SpillAccess::new(&mut session)
        .producer_candidates(0)
        .unwrap();
    assert_eq!(candidates.len(), 4);
    for (address, winners) in [
        ("A3", vec!["A1"]),
        ("A4", vec![]),
        ("D8", vec!["C1"]),
        ("E8", vec![]),
        ("I1", vec!["H1"]),
        ("K1", vec!["H1", "J1"]),
    ] {
        let at: CellAddress = address.parse().unwrap();
        let demand = Area {
            kind: AreaKind::Cell,
            sheet: 0,
            r0: at.row,
            r1: at.row,
            c0: at.col,
            c1: at.col,
        };
        let actual: Vec<_> = candidates
            .iter()
            .filter(|c| c.intersects(demand))
            .map(|c| c.anchor)
            .collect();
        let expected: Vec<CellAddress> = winners.iter().map(|s| s.parse().unwrap()).collect();
        assert_eq!(actual, expected, "{address}");
    }
    assert!(
        SpillAccess::new(&mut session)
            .producer_candidates(1)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn saved_extents_widen_the_candidate_without_trusting_computed_dimensions() {
    for declared in [false, true] {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        let anchor: CellAddress = "A1".parse().unwrap();
        sheet.set_formula(anchor, "SEQUENCE(1)").unwrap();
        sheet
            .set_spill_cached_value("D4".parse().unwrap(), anchor, "SEQUENCE(1)", 7.0.into())
            .unwrap();
        if declared {
            sheet
                .set_spill_extent(anchor, "D4".parse().unwrap(), "SEQUENCE(1)")
                .unwrap();
        }
        let mut session = Session::with_context(&book, EvaluationInputs::default());
        let candidates = SpillAccess::new(&mut session)
            .producer_candidates(0)
            .unwrap();
        let candidate = candidates.first().unwrap();
        assert!(candidate.intersects(Area {
            kind: AreaKind::Cell,
            sheet: 0,
            r0: 3,
            r1: 3,
            c0: 3,
            c1: 3
        }));
        if declared {
            assert_eq!((candidate.area.r1, candidate.area.c1), (3, 3));
        } else {
            assert_eq!(
                (candidate.area.r1, candidate.area.c1),
                (crate::model::MAX_ROWS - 1, crate::model::MAX_COLUMNS - 1)
            );
        }
    }
}
