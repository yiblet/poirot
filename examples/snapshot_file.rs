//! Persist a prepared workbook, restore it, calculate every modeled cell, and write XLSX.
use poirot::{CellValue, Evaluation, EvaluationContext, Spreadsheet, artifact, io};

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

    let mut book = Spreadsheet::new();
    let sheet = book.workbook_mut().add_sheet("Inputs")?;
    sheet.set_value("A1".parse()?, 21.)?;
    sheet.set_formula("B1".parse()?, "A1*2", CellValue::Blank)?;
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
    let mut prepared = Evaluation::with_workbook(book.workbook())
        .with_policy(policy.clone())
        .build(context.clone())?;
    prepared.compile_all();
    let bytes = artifact::encode_workspace(prepared.snapshot(), book.import_warnings(), limits)?;
    // Application file-publication policy is explicit; this example writes new paths.
    std::fs::write(&snapshot_path, bytes)?;
    drop(prepared);

    let restored = artifact::read_file(
        &snapshot_path,
        limits,
        artifact::PreparedPolicy::ReuseCompatible,
    )?;
    eprintln!("prepared state: {:?}", restored.prepared_status);
    let mut workbook = restored.workbook;
    let changes = {
        let mut evaluation = match restored.workspace {
            Some(state) => state.into_evaluation(&workbook, context)?,
            None => Evaluation::with_workbook(&workbook)
                .with_policy(policy)
                .build(context)?,
        };
        // Full modeled-workbook scope refreshes all formula caches before saving.
        evaluation.all()?;
        evaluation.into_changes()
    };
    workbook.apply(changes)?;
    let output = Spreadsheet::from_import(io::ImportReport {
        workbook,
        warnings: restored.import_warnings,
    });
    for warning in output.import_warnings() {
        eprintln!("{warning}");
    }
    output
        .plan_xlsx_save(Default::default())?
        .write_file(&output_path)?;
    let reopened = Spreadsheet::open(&output_path)?;
    let address = "B1".parse()?;
    let saved = reopened
        .workbook()
        .sheet(0)
        .and_then(|sheet| sheet.get(address))
        .ok_or("saved formula cell is missing")?;
    if saved.value != CellValue::Number(42.) {
        return Err("unexpected saved value".into());
    }
    Ok(())
}
