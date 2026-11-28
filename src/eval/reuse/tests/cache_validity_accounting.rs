use super::super::cache::Cache;
use super::super::cached::Cached;
use super::super::validity::Validity;
use crate::eval::reuse::dependencies::Dependencies;
use crate::model::{CellAddress, CellValue};
use std::sync::Arc;

#[test]
fn dirty_storage_accounting_tracks_multiple_duplicate_and_forced_causes() {
    let first = (0, CellAddress { row: 0, col: 0 });
    let second = (0, CellAddress { row: 1, col: 0 });
    let target = (0, CellAddress { row: 0, col: 1 });
    let mut cache = Cache::default();
    for key in [first, second] {
        cache
            .insert(
                key,
                Cached {
                    value: Arc::new(1.0.into()),
                    version: 0,
                    validity: Validity::Clean,
                    dependencies: Box::default(),
                },
            )
            .unwrap();
    }
    let reader = Cached {
        value: Arc::new(CellValue::Text("retained result".into())),
        version: 0,
        validity: Validity::Clean,
        dependencies: Box::new(Dependencies {
            cells: vec![first, second],
            ..Default::default()
        }),
    };
    cache.insert(target, reader).unwrap();
    let clean_bytes = cache.retained_bytes;
    let check = |cache: &Cache| {
        assert_eq!(
            cache.retained_bytes,
            [first, second, target]
                .into_iter()
                .map(|key| cache.retained(key).unwrap().retained_bytes())
                .sum::<usize>()
        );
    };
    cache.invalidate_context([first]);
    check(&cache);
    assert_eq!(cache.retained_bytes, clean_bytes);
    cache.invalidate_context([second]);
    check(&cache);
    assert!(cache.retained_bytes > clean_bytes);
    let multiple_bytes = cache.retained_bytes;
    cache.invalidate_context([first, first, second]);
    check(&cache);
    assert_eq!(cache.retained_bytes, multiple_bytes);
    cache.force(target);
    check(&cache);
    assert_eq!(cache.retained_bytes, clean_bytes);
    cache.force(target);
    check(&cache);
    cache.invalidate_context([first, second]);
    check(&cache);
    assert_eq!(cache.retained_bytes, clean_bytes);
}

#[test]
fn a_conditionally_dirty_spill_reader_cannot_cover_its_own_geometry() {
    use crate::model::query::RangeMembership;

    let root = (0, CellAddress { row: 0, col: 0 });
    let first = (0, CellAddress { row: 1, col: 1 });
    let second = (0, CellAddress { row: 2, col: 2 });
    let range = RangeMembership::new(0, root.1, second.1).unwrap();
    let mut cache = Cache::default();
    for (key, dependencies) in [
        (root, Dependencies::default()),
        (
            first,
            Dependencies {
                cells: vec![root],
                spill_ranges: vec![range],
                spill_sensitive: true,
                ..Default::default()
            },
        ),
        (
            second,
            Dependencies {
                spill_ranges: vec![range],
                spill_sensitive: true,
                ..Default::default()
            },
        ),
    ] {
        cache
            .insert(
                key,
                Cached {
                    version: 0,
                    validity: Validity::Clean,
                    value: Arc::new(CellValue::Number(1.)),
                    dependencies: Box::new(dependencies),
                },
            )
            .unwrap();
    }
    cache.invalidate_context([root]);
    assert!(matches!(
        cache.retained(first).unwrap().validity,
        Validity::Forced
    ));
    assert!(matches!(
        cache.retained(second).unwrap().validity,
        Validity::Forced
    ));
}

#[test]
fn a_later_rectangle_start_requires_its_own_geometry_query() {
    use crate::model::query::RangeMembership;

    let edited = (0, CellAddress { row: 0, col: 0 });
    let anchor = (0, CellAddress { row: 2, col: 2 });
    let reader = (0, CellAddress { row: 4, col: 4 });
    let mut cache = Cache::default();
    for (key, dependencies) in [
        (
            anchor,
            Dependencies {
                cells: vec![edited],
                spill_sensitive: true,
                ..Default::default()
            },
        ),
        (
            reader,
            Dependencies {
                spill_ranges: vec![RangeMembership::new(0, anchor.1, reader.1).unwrap()],
                ..Default::default()
            },
        ),
    ] {
        cache
            .insert(
                key,
                Cached {
                    version: 0,
                    validity: Validity::Clean,
                    value: Arc::new(CellValue::Number(1.)),
                    dependencies: Box::new(dependencies),
                },
            )
            .unwrap();
    }
    cache.invalidate_geometry([edited]);
    assert!(matches!(
        cache.retained(reader).unwrap().validity,
        Validity::Forced
    ));
}

#[test]
fn overlap_between_geometry_and_value_readers_only_forces_that_reader() {
    use crate::model::query::RangeMembership;

    let edit = (0, "A1".parse().unwrap());
    let reader = (0, "B2".parse().unwrap());
    let downstream = (0, "C2".parse().unwrap());
    let geometry = RangeMembership::new(edit.0, edit.1, reader.1).unwrap();
    let mut cache = Cache::default();
    for (key, dependencies) in [
        (
            reader,
            Dependencies {
                cells: vec![edit],
                spill_ranges: vec![geometry],
                ..Default::default()
            },
        ),
        (
            downstream,
            Dependencies {
                cells: vec![reader],
                ..Default::default()
            },
        ),
    ] {
        cache
            .insert(
                key,
                Cached {
                    value: Arc::new(CellValue::Number(1.)),
                    version: 0,
                    validity: Validity::Clean,
                    dependencies: Box::new(dependencies),
                },
            )
            .unwrap();
    }
    let version = cache.retained(reader).unwrap().version;
    let before = cache.retained_bytes;
    cache.invalidate_geometry([edit, edit]);
    assert!(matches!(
        cache.retained(reader).unwrap().validity,
        Validity::Forced
    ));
    assert!(matches!(
        cache.retained(downstream).unwrap().validity,
        Validity::Cause { key, version: dependency } if key == reader && dependency == version
    ));
    assert_eq!(cache.retained_bytes, before);
}
