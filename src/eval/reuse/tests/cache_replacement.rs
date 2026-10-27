use super::super::cache::Cache;
use super::super::cached::Cached;
use super::super::key::Key;
use super::super::validity::Validity;
use crate::eval::reuse::dependencies::Dependencies;
use crate::model::query::RangeMembership;
use crate::model::{CellAddress, CellValue};
use std::sync::Arc;

fn entry(value: f64, source: Key) -> Cached {
    Cached {
        value: Arc::new(CellValue::Number(value)),
        version: 0,
        validity: Validity::Clean,
        dependencies: Box::new(Dependencies {
            cells: vec![source],
            ranges: vec![RangeMembership {
                sheet: source.0,
                first: source.1,
                last: source.1,
            }],
            ..Default::default()
        }),
    }
}

#[test]
fn unchanged_dependencies_preserve_indexes_and_update_result_versions() {
    let target = (0, CellAddress { row: 0, col: 1 });
    let source = (0, CellAddress { row: 0, col: 0 });
    let other = (0, CellAddress { row: 4, col: 0 });
    let mut cache = Cache::default();
    cache.insert(target, entry(1., source)).unwrap();
    cache.rebuild_ranges();
    let old_version = cache.get(target).unwrap().version;
    cache.insert(target, entry(2., source)).unwrap();
    assert!(!cache.ranges_dirty);
    assert!(cache.reverse.get(&source).unwrap().contains(&target));
    assert!(cache.get(target).unwrap().version > old_version);
    let version = cache.get(target).unwrap().version;
    cache.insert(target, entry(2., source)).unwrap();
    assert_eq!(cache.get(target).unwrap().version, version);
    cache.insert(target, entry(3., other)).unwrap();
    assert!(cache.ranges_dirty);
    assert!(!cache.reverse.contains_key(&source));
    assert!(cache.reverse.get(&other).unwrap().contains(&target));
    let mut volatile = entry(4., other);
    volatile.dependencies.volatile = true;
    cache.insert(target, volatile).unwrap();
    assert!(cache.always_dirty.contains(&target));
    cache.insert(target, entry(4., other)).unwrap();
    assert!(!cache.always_dirty.contains(&target));
    cache.invalidate_values([other]);
    assert!(cache.get(target).is_none());
    assert_eq!(
        cache.retained_bytes,
        cache.retained(target).unwrap().retained_bytes()
    );
}
