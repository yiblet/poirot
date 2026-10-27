use super::super::cache::Cache;
use super::super::cached::Cached;
use super::super::validity::Validity;
use crate::eval::limits::EvaluationFailure;

use crate::model::{CellAddress, CellValue};
use std::sync::Arc;

fn entry() -> Cached {
    Cached {
        version: 0,
        validity: Validity::Clean,
        value: Arc::new(CellValue::Number(1.)),
        dependencies: Box::default(),
    }
}

#[test]
fn unlimited_policy_still_refuses_intrinsic_cache_size_overflow_atomically() {
    // Exercise the counter boundary without allocating machine-sized storage.
    let mut cache = Cache {
        retained_bytes: usize::MAX,
        ..Default::default()
    };
    let key = (0, CellAddress { row: 0, col: 0 });
    assert!(matches!(
        cache.commit(
            vec![(key, entry())],
            crate::eval::limits::MemoRetentionLimits::default(),
            None,
            []
        ),
        Err(EvaluationFailure::MemoRetentionLimit { .. })
    ));
    assert!(cache.insert(key, entry()).is_err());
    assert_eq!(cache.retained_bytes, usize::MAX);
    assert_eq!(cache.len(), 0);
}
