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
