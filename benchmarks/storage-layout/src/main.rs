//! Safe storage diagnostics separating first-use work, warmed access and clone lifetimes.
use poirot_model::{Cell, CellAddress, CellStore, CellValue};
use serde_json::{Map, Value, json};
use std::{hint::black_box, time::Instant};

/// Operational errors from constructing or checking a diagnostic workload.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn nanoseconds(start: Instant) -> Value {
    // These bounded probes finish long before u64 nanoseconds would wrap.
    (start.elapsed().as_nanos() as u64).into()
}

fn addresses() -> Vec<(&'static str, Vec<CellAddress>)> {
    vec![
        (
            "dense",
            (0..1024)
                .flat_map(|row| (0..16).map(move |col| CellAddress { row, col }))
                .collect(),
        ),
        (
            "sparse",
            (0..2048)
                .map(|i| {
                    // SAFETY: i < 2048 bounds i*64 by 131008 and (i%128)*8 by 1016; both fit u32.
                    CellAddress {
                        row: i * 64,
                        col: (i % 128) * 8,
                    }
                })
                .collect(),
        ),
        (
            "striped",
            (0..2048)
                .flat_map(|row| [0, 4].map(move |col| CellAddress { row, col }))
                .collect(),
        ),
    ]
}

fn fill(addresses: &[CellAddress]) -> Result<CellStore> {
    let mut store = CellStore::default();
    for address in addresses {
        store.set(*address, Cell::default())?;
    }
    Ok(store)
}

fn check_count(store: &CellStore, expected: usize) -> Result<()> {
    if store.cells().iter().count() != expected {
        return Err("stored coordinate count differs from generated input".into());
    }
    Ok(())
}

fn measure(addresses: &[CellAddress]) -> Result<Value> {
    let mut report = Map::new();
    let mut store = CellStore::<Cell>::default();
    let mut failure = None;
    let start = Instant::now();
    let allocation = allocation_counter::measure(|| {
        for (index, address) in addresses.iter().enumerate() {
            if let Err(error) = store.set(*address, Cell::new(index as f64)) {
                failure = Some(error);
                break;
            }
        }
    });
    report.insert("insert_row_ns".into(), nanoseconds(start));
    if let Some(error) = failure {
        return Err(error.into());
    }
    report.insert("retained_bytes".into(), allocation.bytes_current.into());
    report.insert("insert_allocations".into(), allocation.count_total.into());
    report.insert("cells".into(), addresses.len().into());

    let start = Instant::now();
    check_count(&store, addresses.len())?;
    report.insert("first_row_ns".into(), nanoseconds(start));
    let start = Instant::now();
    for _ in 0..8 {
        black_box(
            store
                .cells()
                .iter()
                .fold(0u64, |sum, (a, _)| sum.wrapping_add(u64::from(a.row))),
        );
    }
    report.insert("warm_rows_ns".into(), nanoseconds(start));

    // A different store keeps the column's first access independent of row preparation.
    let cold = fill(addresses)?;
    let start = Instant::now();
    let count = black_box(cold.cells().column(0, 0, 1_048_575).count());
    report.insert("first_column_ns".into(), nanoseconds(start));
    if count != addresses.iter().filter(|address| address.col == 0).count() {
        return Err("column scan differs from generated coordinates".into());
    }
    black_box(store.cells().column(0, 0, 1_048_575).count());
    let start = Instant::now();
    for _ in 0..16 {
        black_box(store.cells().column(0, 0, 1_048_575).count());
    }
    report.insert("warm_columns_ns".into(), nanoseconds(start));

    let start = Instant::now();
    for _ in 0..32 {
        for address in addresses {
            black_box(store.get(black_box(address)));
        }
    }
    report.insert("point_reads_ns".into(), nanoseconds(start));
    // Correctness checks remain outside the timed lookup loop.
    for (index, address) in addresses.iter().enumerate() {
        if store.get(address).map(|cell| &cell.value) != Some(&CellValue::Number(index as f64)) {
            return Err("point lookup differs from independently assigned input".into());
        }
    }

    let start = Instant::now();
    for _ in 0..16 {
        // Destruction is intentionally timed here: allocation reuse is part of this lifecycle.
        black_box(store.clone());
    }
    report.insert("immediate_clone_drop_ns".into(), nanoseconds(start));

    // Separately retain four clones so construction is not confused with immediate
    // destruction/reuse. Their first query and destruction have their own timers.
    let mut clones = Vec::new();
    let start = Instant::now();
    let cloned = allocation_counter::measure(|| {
        for _ in 0..4 {
            clones.push(black_box(store.clone()));
        }
    });
    report.insert("retained_clone_construction_ns".into(), nanoseconds(start));
    report.insert("clone_retained_bytes".into(), cloned.bytes_current.into());
    report.insert("clone_allocations".into(), cloned.count_total.into());
    let start = Instant::now();
    for clone in &clones {
        check_count(clone, addresses.len())?;
    }
    report.insert("clones_first_rows_ns".into(), nanoseconds(start));
    let start = Instant::now();
    drop(clones);
    report.insert("retained_clone_drop_ns".into(), nanoseconds(start));

    let mut columns = addresses.to_vec();
    columns.sort_unstable_by_key(|address| (address.col, address.row));
    let start = Instant::now();
    let column_store = fill(&columns)?;
    report.insert("insert_column_ns".into(), nanoseconds(start));
    black_box(&column_store);
    check_count(&column_store, addresses.len())?;
    Ok(Value::Object(report))
}

fn main() -> Result<()> {
    let mut workloads = Map::new();
    for (name, addresses) in addresses() {
        workloads.insert(name.into(), measure(&addresses)?);
    }
    println!("{}", json!({"workloads":workloads}));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn workloads_have_known_membership_and_valid_distinct_addresses() {
        let workloads = addresses();
        assert_eq!(
            workloads.iter().map(|(_, a)| a.len()).collect::<Vec<_>>(),
            [16384, 2048, 4096]
        );
        for (_, coordinates) in workloads {
            let unique: BTreeSet<_> = coordinates.iter().copied().collect();
            assert_eq!(unique.len(), coordinates.len());
            assert!(coordinates.iter().all(|address| address.validate().is_ok()));
            let store = fill(&coordinates).unwrap();
            assert_eq!(
                store.cells().keys().copied().collect::<BTreeSet<_>>(),
                unique
            );
        }
    }
}
