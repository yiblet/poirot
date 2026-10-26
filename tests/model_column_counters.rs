//! Column metrics count the column bound seek, loaded row pages and stored slots.
use poirot::model::{Cell, CellAddress, CellStore, query::QueryCounters};

#[test]
fn counters_follow_pages_and_stored_slots_without_visiting_unrelated_columns() {
    let mut cells = CellStore::<Cell>::default();
    for (row, col) in [(0, 0), (31, 0), (32, 0), (1000, 4)] {
        cells
            .set(CellAddress { row, col }, Cell::default())
            .unwrap();
    }
    let mut column = cells.cells().column(0, 0, 40);
    assert_eq!(
        column.counters(),
        QueryCounters {
            column_seeks: 1,
            ..Default::default()
        }
    );
    for (row, pages, visits) in [(0, 1, 1), (31, 1, 2), (32, 2, 3)] {
        assert_eq!(column.next().unwrap().0.row, row);
        assert_eq!(
            column.counters(),
            QueryCounters {
                row_seeks: pages,
                column_seeks: 1,
                cells_visited: visits,
                key_lookups: 0,
            }
        );
    }
    let exhausted = column.counters();
    assert!(column.next().is_none());
    assert!(column.next().is_none());
    assert_eq!(column.counters(), exhausted);
}

#[test]
fn empty_columns_charge_only_the_indexes_they_consult() {
    let mut cells = CellStore::<Cell>::default();
    cells
        .set(CellAddress { row: 5, col: 1 }, Cell::default())
        .unwrap();
    for (col, first, last, pages, bound) in [(0, 0, 9, 1, 1), (4, 0, 9, 0, 1), (0, 10, 0, 0, 0)] {
        let mut column = cells.cells().column(col, first, last);
        assert!(column.next().is_none());
        assert!(column.next().is_none());
        assert_eq!(
            column.counters(),
            QueryCounters {
                row_seeks: pages,
                column_seeks: bound,
                ..Default::default()
            }
        );
    }
}
