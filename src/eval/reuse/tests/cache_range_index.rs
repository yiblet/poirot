use super::super::cache::Cache;
use super::super::cached::Cached;
use super::super::range_index::RangeIndex;
use super::super::validity::Validity;
use crate::eval::reuse::dependencies::Dependencies;
use crate::model::query::RangeMembership;
use crate::model::{CellAddress, CellValue};
use std::sync::Arc;

#[test]
fn consuming_matches_keeps_row_zero_and_column_misses_live() {
    let a = (0, CellAddress { row: 0, col: 0 });
    let c = (0, CellAddress { row: 0, col: 2 });
    let reader = (1, CellAddress { row: 0, col: 0 });
    let other = (1, CellAddress { row: 1, col: 0 });
    let mut cache = Cache::default();
    for (key, source) in [(reader, a), (reader, c), (other, c)] {
        let mut deps = cache
            .get(key)
            .map(|e| (*e.dependencies).clone())
            .unwrap_or_default();
        deps.ranges
            .push(RangeMembership::new(0, source.1, source.1).unwrap());
        cache
            .insert(
                key,
                Cached {
                    version: 0,
                    validity: Validity::Clean,
                    value: Arc::new(1.0.into()),
                    dependencies: Box::new(deps),
                },
            )
            .unwrap();
    }
    let mut index = RangeIndex::new(&cache.entries);
    let mut found = Vec::new();
    // B1 lies inside global column bounds, but matches no rectangle.
    index.take_readers((0, CellAddress { row: 0, col: 1 }), &mut found);
    assert!(found.is_empty());
    index.take_readers(a, &mut found);
    assert_eq!(found, [reader]);
    found.clear();
    index.take_readers(a, &mut found);
    assert!(found.is_empty());
    index.take_readers(c, &mut found);
    found.sort();
    assert_eq!(found, [reader, other]);
    found.clear();
    index.take_readers(c, &mut found);
    assert!(found.is_empty());
    assert!(
        index
            .sheets
            .values()
            .all(|t| t.max_end.get(1) == Some(&None))
    );

    // A real pass removes the first reader; the second survives and its
    // column-C entry must be reconstructed for a later independent pass.
    cache.invalidate([a, a, reader]);
    assert!(cache.get(reader).is_none());
    assert!(cache.get(other).is_some());
    cache.invalidate([c]);
    assert_eq!(cache.len(), 0);
}

#[test]
fn duplicate_points_ranges_and_cycles_preserve_transitive_closure() {
    let keys = (0..4)
        .map(|row| (0, CellAddress { row, col: 0 }))
        .collect::<Vec<_>>();
    let mut cache = Cache::default();
    // 0 -> 1 -> 2 -> 1 is a cycle, and 2 -> 3 closes the chain.
    // Each arrow has both a point edge and two separate rectangle entries.
    for (consumer, source) in [(1, 0), (2, 1), (1, 2), (3, 2)] {
        let mut deps = cache
            .get(keys[consumer])
            .map(|e| (*e.dependencies).clone())
            .unwrap_or_default();
        deps.cells.push(keys[source]);
        let range = RangeMembership::new(0, keys[source].1, keys[source].1).unwrap();
        deps.ranges.push(range);
        deps.point_runs.push(range);
        cache
            .insert(
                keys[consumer],
                Cached {
                    version: 0,
                    validity: Validity::Clean,
                    value: Arc::new(1.0.into()),
                    dependencies: Box::new(deps),
                },
            )
            .unwrap();
    }
    cache.invalidate([keys[0], keys[1], keys[0]]);
    assert_eq!(cache.len(), 0);
    cache.invalidate([keys[0]]);
    assert_eq!(cache.len(), 0);
}

#[test]
fn indexed_readers_match_rectangle_membership_on_both_axes_and_sheets() {
    // Plain inclusive inequalities form the independent rectangle oracle.
    // Include column gaps within global bounds, points, overlapping rectangles,
    // transposes, a blank-dependency rectangle and a second source sheet.
    let rectangles = [
        (0, 0, 0, 5, 0),
        (0, 0, 3, 5, 3),
        (0, 3, 0, 3, 5),
        (0, 1, 1, 4, 4),
        (0, 7, 7, 7, 7),
        (1, 0, 0, 5, 0),
    ];
    let mut cache = Cache::default();
    for (i, &(sheet, top, left, bottom, right)) in rectangles.iter().enumerate() {
        let range = RangeMembership::new(
            sheet,
            CellAddress {
                row: top,
                col: left,
            },
            CellAddress {
                row: bottom,
                col: right,
            },
        )
        .unwrap();
        let mut dependencies = Dependencies::default();
        if i % 2 == 0 {
            dependencies.ranges.push(range);
        } else {
            dependencies.point_runs.push(range);
        }
        cache
            .insert(
                (
                    2,
                    CellAddress {
                        row: i as u32,
                        col: 9,
                    },
                ),
                Cached {
                    version: 0,
                    validity: Validity::Clean,
                    value: Arc::new(CellValue::Number(i as f64)),
                    dependencies: Box::new(dependencies),
                },
            )
            .unwrap();
    }
    for sheet in 0..4 {
        for row in 0..10 {
            for col in 0..10 {
                let mut actual = Vec::new();
                let mut index = RangeIndex::new(&cache.entries);
                let key = (sheet, CellAddress { row, col });
                let mut retained = Vec::new();
                index.readers(key, &mut retained);
                assert_eq!(index.intersects(key), !retained.is_empty());
                // Read-only certificates must survive repeated queries.
                let mut repeated = Vec::new();
                index.readers(key, &mut repeated);
                assert_eq!(retained, repeated);
                index.take_readers(key, &mut actual);
                retained.sort_unstable();
                actual.sort_unstable();
                assert_eq!(retained, actual);
                actual.sort();
                let expected = rectangles
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &(s, t, l, b, r))| {
                        (s == sheet && t <= row && row <= b && l <= col && col <= r).then_some((
                            2,
                            CellAddress {
                                row: i as u32,
                                col: 9,
                            },
                        ))
                    })
                    .collect::<Vec<_>>();
                assert_eq!(actual, expected, "sheet={sheet}, row={row}, col={col}");
            }
        }
    }
    let mut actual = Vec::new();
    RangeIndex::default().take_readers((0, CellAddress { row: 0, col: 0 }), &mut actual);
    assert!(actual.is_empty());
}
