//! Save a generated model as a native snapshot, restore it, and export calculated XLSX.
//! Run with MODEL.poirot OUTPUT.xlsx; both files are created by this example.
use poirot::{CellValue, Evaluation, EvaluationContext, Workbook, artifact};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let snapshot_path = args
        .next()
        .ok_or("usage: snapshot_file MODEL.poirot OUTPUT.xlsx")?;
    let output_path = args
        .next()
        .ok_or("usage: snapshot_file MODEL.poirot OUTPUT.xlsx")?;
    if args.next().is_some() {
        return Err("usage: snapshot_file MODEL.poirot OUTPUT.xlsx".into());
    }

    let mut book = Workbook::new();
    let sheet = book.add_sheet("Inputs")?;
    sheet.set_value("A1".parse()?, 21.)?;
    sheet.set_formula("B1".parse()?, "A1*2")?;
    let limits = artifact::SnapshotLimits::default();
    let context = EvaluationContext {
        as_of_time: std::time::UNIX_EPOCH,
        randomness_seed: 42,
        ..Default::default()
    };
    let policy = poirot::OverlayPolicy {
        builtin: poirot::eval::BuiltinSemanticsPolicy::ExcelDocumentation,
        ..Default::default()
    };
    let mut prepared = Evaluation::with_workbook(&book)
        .with_policy(policy.clone())
        .build(context.clone())?;
    prepared.compile_all();
    prepared
        .save_snapshot()
        .limits(limits)
        .write_file(&snapshot_path)?;
    drop(prepared);

    let artifact::RestoredEvaluation {
        mut evaluation,
        prepared_status,
    } = Workbook::load_snapshot()
        .limits(limits)
        .open(&snapshot_path)?
        .into_evaluation(context, policy)?;
    eprintln!("prepared state: {prepared_status:?}");
    // Refresh saved formula results explicitly before exporting the source.
    evaluation.all()?;
    let mut output = Workbook::from_model(evaluation.workbook().clone());
    output.apply(evaluation.into_changes())?;
    if let Some(metadata) = &output.file_metadata {
        for warning in &metadata.import_warnings {
            eprintln!("{warning}");
        }
    }
    output.save_xlsx().write_file(&output_path)?;
    let reopened = Workbook::open(&output_path)?;
    let address = "B1".parse()?;
    let saved = reopened
        .sheet(0)
        .and_then(|sheet| sheet.get(address))
        .ok_or("saved formula cell is missing")?;
    if saved.value != CellValue::Number(42.) {
        return Err("unexpected saved value".into());
    }
    Ok(())
}
