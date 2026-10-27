//! Compare retained incremental overlays with actual native snapshot file restoration.
use super::*;
use poirot::eval::artifact::RestoredCheckpoint;
use poirot::snapshots::{PreparedPolicy, PreparedStatus};
use std::{
    borrow::Borrow,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

/// Private scratch directory removed on success and recoverable failures.
struct Scratch(PathBuf);
impl Scratch {
    /// Reserve a private child without trusting or changing the parent permissions.
    fn create(parent: &Path) -> Result<Self> {
        fs::create_dir_all(parent)?;
        for nonce in 0..1000 {
            let path = parent.join(format!("snapshot-{}-{nonce}", std::process::id()));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err("unable to reserve private snapshot directory".into())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Evaluate one cycle, timing execution and publication but excluding hashes and checks.
fn evaluate<W: Borrow<poirot::model::Workbook>>(
    overlay: &mut Overlay<'_, W>,
    request: &Scenario,
) -> Result<Value> {
    let began = Instant::now();
    let mut cycle = overlay.evaluation_with_profiling(&request.context, true)?;
    let values = cycle.outputs(&request.outputs)?;
    let report = cycle.finish();
    report.result?;
    let evaluation_ms = milliseconds(began);
    let usage = report.usage.ok_or("missing profiling work counters")?;
    let execution = report.execution.ok_or("missing profiling counters")?;
    let formulas = execution.formula_executions;
    if let Some(expected) = &request.expected
        && !values.iter().map(|cell| &cell.value).eq(expected.iter())
    {
        return Err("synthetic outputs disagree with independent mathematical control".into());
    }
    Ok(
        json!({"evaluation_ms": evaluation_ms, "formula_executions": formulas,
        "work_units": usage.work_units, "cell_reads": usage.cell_reads,
        "sparse_entries": usage.sparse_entries, "comparisons": usage.comparisons,
        "lookup_prefix_hits": execution.lookup_prefix_hits,
        "lookup_prefix_scans": execution.lookup_prefix_scans,
        "signature": signature(&values)?, "outputs": values.len(),
        "errors": values.iter().filter(|cell| matches!(cell.value, CellValue::Error(_))).count()}),
    )
}

/// Apply only differences between independent scenarios, including omitted-input restoration.
fn edit<W: Borrow<poirot::model::Workbook>>(
    overlay: &mut Overlay<'_, W>,
    book: &Workbook,
    previous: &BTreeMap<Key, Input>,
    request: &Scenario,
) -> Result<f64> {
    let began = Instant::now();
    for &key in previous
        .keys()
        .filter(|key| !request.inputs.contains_key(key))
    {
        restore(overlay, book, key)?;
    }
    for (&key, input) in &request.inputs {
        if previous.get(&key) != Some(input) {
            apply(overlay, book, key, input)?;
        }
    }
    Ok(milliseconds(began))
}

/// Reject mismatched outputs without exposing private values in errors.
fn agree(retained: &Value, restored: &Value) -> Result<()> {
    for key in ["signature", "errors", "outputs"] {
        if retained.get(key) != restored.get(key) {
            return Err(format!("snapshot comparison disagrees on {key}").into());
        }
    }
    Ok(())
}

/// Alternate paired evaluation order to reduce systematic cache and scheduling bias.
fn paired<A: Borrow<poirot::model::Workbook>, B: Borrow<poirot::model::Workbook>>(
    retained: &mut Overlay<'_, A>,
    restored: &mut Overlay<'_, B>,
    request: &Scenario,
    reverse: bool,
) -> Result<Value> {
    let (a, b) = if reverse {
        let b = evaluate(restored, request)?;
        (evaluate(retained, request)?, b)
    } else {
        let a = evaluate(retained, request)?;
        (a, evaluate(restored, request)?)
    };
    agree(&a, &b)?;
    Ok(json!({"retained": a, "restored": b,
        "order": if reverse {"restored_first"} else {"retained_first"}}))
}

/// Save once after warming, restore real incremental state, then replay matching cycles.
fn measure(book: &Workbook, requests: &[Scenario], parent: &Path) -> Result<Value> {
    let first = requests
        .first()
        .ok_or("snapshot mode needs at least one request")?;
    let scratch = Scratch::create(parent)?;
    let path = scratch.0.join("state.snapshot");
    let began = Instant::now();
    let mut retained = book.overlay().build()?;
    let setup_ms = milliseconds(began);
    let first_edit_ms = edit(&mut retained, book, &BTreeMap::new(), first)?;
    let first_evaluation = evaluate(&mut retained, first)?;
    let began = Instant::now();
    let encoded = retained.save_snapshot().to_buffer()?;
    let snapshot_encode_ms = milliseconds(began);
    let snapshot_bytes = encoded.len();
    let began = Instant::now();
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    file.write_all(&encoded)?;
    drop(file);
    let snapshot_write_ms = milliseconds(began);
    drop(encoded);
    let began = Instant::now();
    let encoded = fs::read(&path)?;
    let snapshot_read_ms = milliseconds(began);
    let began = Instant::now();
    let artifact = Workbook::restore_snapshot()
        .prepared_policy(PreparedPolicy::RequireCompatible)
        .from_bytes(&encoded)?;
    if artifact.prepared_status() != PreparedStatus::Restored {
        return Err("snapshot preparation was not restored".into());
    }
    let RestoredCheckpoint::Prepared(core) = artifact.into_artifact().into_checkpoint()? else {
        return Err("snapshot did not restore an incremental overlay".into());
    };
    let mut restored = Overlay::from_overlay(*core);
    let snapshot_decode_attach_ms = milliseconds(began);
    drop(encoded);
    let mut pairs = Vec::new();
    let mut previous = &first.inputs;
    for (index, request) in requests.iter().enumerate() {
        if index != 0 {
            let retained_edit_ms = edit(&mut retained, book, previous, request)?;
            let restored_edit_ms = edit(&mut restored, book, previous, request)?;
            let mut pair = paired(
                &mut retained,
                &mut restored,
                request,
                index.is_multiple_of(2),
            )?;
            let fields = pair.as_object_mut().ok_or("invalid pair report")?;
            fields.insert("phase".into(), json!("next_scenario"));
            fields.insert("request_index".into(), json!(index));
            fields.insert("retained_edit_ms".into(), json!(retained_edit_ms));
            fields.insert("restored_edit_ms".into(), json!(restored_edit_ms));
            pairs.push(pair);
        }
        let mut pair = paired(
            &mut retained,
            &mut restored,
            request,
            !index.is_multiple_of(2),
        )?;
        let fields = pair.as_object_mut().ok_or("invalid pair report")?;
        fields.insert("phase".into(), json!("unchanged"));
        fields.insert("request_index".into(), json!(index));
        pairs.push(pair);
        previous = &request.inputs;
    }
    Ok(json!({"setup_ms": setup_ms, "first_edit_ms": first_edit_ms,
        "first_evaluation": first_evaluation, "snapshot_bytes": snapshot_bytes,
        "snapshot_encode_ms": snapshot_encode_ms, "snapshot_write_ms": snapshot_write_ms,
        "snapshot_read_ms": snapshot_read_ms, "snapshot_decode_attach_ms": snapshot_decode_attach_ms,
        "prepared_status": "restored", "pairs": pairs}))
}

/// Load bounded replay inputs and emit only timings, work counters and value hashes.
pub(super) fn run(args: &[String]) -> Result<()> {
    let input = args.get(1).ok_or("missing workbook or --synthetic")?;
    let requests = args
        .get(2)
        .ok_or("missing requests file or synthetic case")?;
    let limit = args.get(3).map(|v| v.parse()).transpose()?.unwrap_or(10);
    let parent = args.get(4).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/replay-snapshots")
    });
    let mut file_read_ms = 0.;
    let mut import_ms = 0.;
    let began = Instant::now();
    let (book, scenarios) = if input == "--synthetic" {
        synthetic(requests, limit)?
    } else {
        let load = Instant::now();
        let bytes = fs::read(input)?;
        file_read_ms = milliseconds(load);
        let load = Instant::now();
        let options = poirot::files::ReadOptions {
            max_input_bytes: u64::MAX,
            max_zip_entry_bytes: u64::MAX,
            max_zip_total_bytes: u64::MAX,
            max_zip_entries: usize::MAX,
            ..Default::default()
        };
        let book = Workbook::load().options(options).from_bytes(&bytes)?;
        import_ms = milliseconds(load);
        let scenarios = load_requests(&book, requests, limit)?;
        (book, scenarios)
    };
    let preparation_ms = milliseconds(began);
    let mut report = measure(&book, &scenarios, &parent)?;
    let fields = report.as_object_mut().ok_or("invalid snapshot report")?;
    fields.insert("file_read_ms".into(), json!(file_read_ms));
    fields.insert("import_ms".into(), json!(import_ms));
    fields.insert("input_preparation_ms".into(), json!(preparation_ms));
    fields.insert("mode".into(), json!("snapshot"));
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_preserve_independent_controls_and_incremental_work() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/snapshot-tests");
        for name in ["diamond", "branches", "spill"] {
            let scratch = Scratch::create(&root).unwrap();
            let (book, scenarios) = synthetic(name, 4).unwrap();
            let report = measure(&book, &scenarios, &scratch.0).unwrap();
            assert!(report["snapshot_bytes"].as_u64().unwrap() > 0);
            for pair in report["pairs"].as_array().unwrap() {
                let a = &pair["retained"];
                let b = &pair["restored"];
                assert_eq!(a["signature"], b["signature"], "{name}");
                assert_eq!(a["errors"], 0);
                assert_eq!(
                    a["formula_executions"], b["formula_executions"],
                    "{name} {pair}"
                );
                for counter in [
                    "work_units",
                    "cell_reads",
                    "sparse_entries",
                    "comparisons",
                    "lookup_prefix_hits",
                    "lookup_prefix_scans",
                ] {
                    assert_eq!(a[counter], b[counter], "{name} {counter}");
                }
                if pair["phase"] == "unchanged" {
                    assert_eq!(a["formula_executions"], 0, "{name}");
                }
            }
            assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 0);
        }
    }

    #[test]
    fn scratch_is_private_and_cleans_up_on_error() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/snapshot-tests");
        let scratch = Scratch::create(&root).unwrap();
        let path = scratch.0.clone();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        let blocked = path.join("not-a-directory");
        fs::write(&blocked, []).unwrap();
        let (book, scenarios) = synthetic("branches", 1).unwrap();
        assert!(measure(&book, &scenarios, &blocked).is_err());
        drop(scratch);
        assert!(!path.exists());
    }
}
