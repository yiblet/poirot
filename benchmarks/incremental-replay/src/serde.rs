//! Compare XLSX and native snapshot serialization of one local workbook.
//! Reports contain only timings, sizes, counts and allocation totals.
use super::*;
use poirot::eval::artifact::RestoredCheckpoint;
use poirot::model::snapshot::Snapshot;
use poirot::snapshots::{PreparedPolicy, PreparedStatus};
use std::hint::black_box;

/// Allocation totals for one untimed run of an operation.
#[derive(Clone, Copy, Default)]
struct Allocations {
    count: u64,
    bytes: u64,
    peak_bytes: u64,
}

/// Run one operation while counting heap allocations when the feature is enabled.
#[cfg(feature = "count-allocations")]
fn allocations<T>(operation: &mut impl FnMut() -> Result<T>) -> Result<Option<Allocations>> {
    let mut outcome = Ok(());
    let info = allocation_counter::measure(|| {
        outcome = operation().map(drop);
    });
    outcome?;
    Ok(Some(Allocations {
        count: info.count_total,
        bytes: info.bytes_total,
        peak_bytes: info.bytes_max,
    }))
}

/// Allocation counting is disabled; timing runs use the system allocator unchanged.
#[cfg(not(feature = "count-allocations"))]
fn allocations<T>(_: &mut impl FnMut() -> Result<T>) -> Result<Option<Allocations>> {
    Ok(None)
}

/// Middle sample, or the mean of the two middle samples.
fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    let middle = samples.len() / 2;
    let upper = samples.get(middle).copied().unwrap_or(f64::NAN);
    match middle.checked_sub(1).and_then(|lower| samples.get(lower)) {
        Some(lower) if samples.len().is_multiple_of(2) => (lower + upper) / 2.,
        _ => upper,
    }
}

/// Time an operation after one warm-up run. Dropping its output is excluded.
fn measure<T>(
    name: &str,
    repetitions: usize,
    bytes: Option<usize>,
    mut operation: impl FnMut() -> Result<T>,
) -> Result<Value> {
    drop(black_box(operation()?));
    let mut samples = Vec::with_capacity(repetitions);
    for _ in 0..repetitions {
        let began = Instant::now();
        let output = black_box(operation()?);
        samples.push(milliseconds(began));
        drop(output);
    }
    let min = samples.iter().copied().fold(f64::INFINITY, f64::min);
    let median_ms = median(&mut samples);
    let counted = allocations(&mut operation)?;
    eprintln!(
        "{name:<40} {median_ms:>10.2} ms {:>12} B {:>12} allocs",
        bytes.map_or("-".into(), |b| b.to_string()),
        counted.map_or("-".into(), |a| a.count.to_string()),
    );
    Ok(
        json!({"operation": name, "median_ms": median_ms, "min_ms": min,
        "repetitions": repetitions, "bytes": bytes,
        "allocations": counted.map(|a| a.count),
        "allocated_bytes": counted.map(|a| a.bytes),
        "peak_allocated_bytes": counted.map(|a| a.peak_bytes)}),
    )
}

/// Measure save, restore and public stages for one prepared checkpoint.
fn checkpoint(
    name: &str,
    repetitions: usize,
    overlay: &poirot::calculation::Overlay<'_>,
    rows: &mut Vec<Value>,
) -> Result<()> {
    let encoded = overlay.save_snapshot().to_buffer()?;
    rows.push(measure(
        &format!("snapshot.{name}.save"),
        repetitions,
        Some(encoded.len()),
        || Ok(overlay.save_snapshot().to_buffer()?),
    )?);
    rows.push(measure(
        &format!("snapshot.{name}.restore"),
        repetitions,
        Some(encoded.len()),
        || {
            let restored = Workbook::restore_snapshot()
                .prepared_policy(PreparedPolicy::RequireCompatible)
                .from_bytes(&encoded)?;
            if restored.prepared_status() != PreparedStatus::Restored {
                return Err("snapshot preparation was not restored".into());
            }
            Ok(restored)
        },
    )?);
    rows.push(measure(
        &format!("snapshot.{name}.restore_attach"),
        repetitions,
        Some(encoded.len()),
        || {
            let restored = Workbook::restore_snapshot()
                .prepared_policy(PreparedPolicy::RequireCompatible)
                .from_bytes(&encoded)?;
            let RestoredCheckpoint::Prepared(core) = restored.into_artifact().into_checkpoint()?
            else {
                return Err("snapshot did not restore an incremental overlay".into());
            };
            Ok(poirot::calculation::Overlay::from_overlay(*core))
        },
    )?);
    rows.push(measure(
        &format!("snapshot.{name}.restore_source_only"),
        repetitions,
        Some(encoded.len()),
        || {
            Ok(Workbook::restore_snapshot()
                .prepared_policy(PreparedPolicy::SourceOnly)
                .from_bytes(&encoded)?)
        },
    )?);
    Ok(())
}

/// Load one workbook, then measure each serialization path with medians.
pub(super) fn run(args: &[String]) -> Result<()> {
    let input = args
        .get(1)
        .ok_or("usage: replay serde workbook.xlsx [repetitions]")?;
    let repetitions = args.get(2).map(|v| v.parse()).transpose()?.unwrap_or(7);
    if repetitions == 0 {
        return Err("repetitions must be positive".into());
    }
    let xlsx = std::fs::read(input)?;
    let options = poirot::files::ReadOptions {
        max_input_bytes: u64::MAX,
        max_zip_entry_bytes: u64::MAX,
        max_zip_total_bytes: u64::MAX,
        max_zip_entries: usize::MAX,
        ..Default::default()
    };
    let write = poirot::files::WriteOptions {
        max_output_bytes: u64::MAX,
        max_zip_entry_bytes: u64::MAX,
        max_zip_total_bytes: u64::MAX,
        max_zip_entries: usize::MAX,
        max_source_bytes: u64::MAX,
    };
    let open = |bytes: &[u8]| -> Result<Workbook> {
        Ok(Workbook::load().options(options).from_bytes(bytes)?)
    };
    let book = open(&xlsx)?;
    let mut rows = Vec::new();
    rows.push(measure("xlsx.open", repetitions, Some(xlsx.len()), || {
        open(&xlsx)
    })?);
    // Report an export refusal as a row instead of aborting the remaining measurements.
    match book.save_xlsx().options(write).to_buffer() {
        Ok(saved) => {
            rows.push(measure(
                "xlsx.save",
                repetitions,
                Some(saved.len()),
                || Ok(book.save_xlsx().options(write).to_buffer()?),
            )?);
            rows.push(measure(
                "xlsx.reopen_saved",
                repetitions,
                Some(saved.len()),
                || open(&saved),
            )?);
        }
        Err(error) => {
            eprintln!("xlsx.save refused: {error}");
            rows.push(json!({"operation": "xlsx.save", "error": error.to_string()}));
        }
    }

    let source = book.save_snapshot().to_buffer()?;
    rows.push(measure(
        "snapshot.source.save",
        repetitions,
        Some(source.len()),
        || Ok(book.save_snapshot().to_buffer()?),
    )?);
    rows.push(measure(
        "snapshot.source.restore",
        repetitions,
        Some(source.len()),
        || Ok(Workbook::restore_snapshot().from_bytes(&source)?),
    )?);
    // Public model-snapshot stages, without the file envelope or bounded decoder.
    let model = &book;
    let wire = postcard::to_allocvec(&Snapshot::capture(model))?;
    rows.push(measure("stage.source.capture", repetitions, None, || {
        Ok(Snapshot::capture(model))
    })?);
    let captured = Snapshot::capture(model);
    rows.push(measure(
        "stage.source.postcard_encode",
        repetitions,
        Some(wire.len()),
        || Ok(postcard::to_allocvec(&captured)?),
    )?);
    drop(captured);
    rows.push(measure(
        "stage.source.postcard_decode_unbounded",
        repetitions,
        Some(wire.len()),
        || Ok(postcard::from_bytes::<Snapshot<'static>>(&wire)?),
    )?);
    let mut decoded = Vec::new();
    // One warm-up, the timed runs and one allocation-counting run each consume a decode.
    for _ in 0..repetitions.saturating_add(2) {
        decoded.push(postcard::from_bytes::<Snapshot<'static>>(&wire)?);
    }
    rows.push(measure(
        "stage.source.model_restore",
        repetitions,
        None,
        || {
            let snapshot = decoded.pop().ok_or("decoded snapshots exhausted")?;
            Ok(snapshot.restore()?)
        },
    )?);
    rows.push(measure("stage.source.matches", repetitions, None, || {
        Ok(Snapshot::capture(model).matches(model))
    })?);

    let context = EvaluationContext::default();
    let began = Instant::now();
    let mut overlay = book.overlay().build()?;
    let build_ms = milliseconds(began);
    let began = Instant::now();
    let usage = overlay.compile_all();
    let compile_ms = milliseconds(began);
    checkpoint("compiled", repetitions, &overlay, &mut rows)?;
    let began = Instant::now();
    let mut cycle = overlay.evaluation(&context)?;
    let cells = cycle.all()?.len();
    cycle.finish().result?;
    let evaluate_ms = milliseconds(began);
    let cached = overlay.cached_cells();
    checkpoint("warm", repetitions, &overlay, &mut rows)?;
    println!(
        "{}",
        json!({"mode": "serde", "workbook_bytes": xlsx.len(),
        "sheets": book.sheets().len(),
        "cells": book.sheets().iter().map(|s| s.cells().len()).sum::<usize>(),
        "overlay_build_ms": build_ms, "compile_all_ms": compile_ms,
        "parse_attempts": usage.parse_attempts,
        "unique_programs": usage.unique_programs, "first_evaluation_ms": evaluate_ms,
        "evaluated_cells": cells, "cached_cells": cached,
        "allocation_counting": cfg!(feature = "count-allocations"),
        "operations": rows})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn median_handles_odd_and_even_samples() {
        assert_eq!(median(&mut [3., 1., 2.]), 2.);
        assert_eq!(median(&mut [4., 1., 2., 3.]), 2.5);
    }

    #[test]
    fn synthetic_workbook_round_trips_through_every_path() {
        let (book, _) = synthetic("spill", 1).unwrap();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/serde-tests");
        std::fs::create_dir_all(&path).unwrap();
        let file = path.join(format!("book-{}.xlsx", std::process::id()));
        book.save_xlsx().write_file(&file).unwrap();
        let outcome = run(&["serde".into(), file.display().to_string(), "1".into()]);
        std::fs::remove_file(&file).unwrap();
        outcome.unwrap();
    }
}
