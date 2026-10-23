//! Measure workbook loading without evaluation or serialization.
use poirot::Workbook;
use std::{io::Write, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or("usage: load_workbook INPUT [REPEATS]")?;
    let repeats = args
        .next()
        .map(|n| n.to_string_lossy().parse::<usize>())
        .transpose()?
        .unwrap_or(5);
    if repeats == 0 || args.next().is_some() {
        return Err("usage: load_workbook INPUT [REPEATS] (REPEATS must be positive)".into());
    }
    for run in 1..=repeats {
        let started = Instant::now();
        let workbook = Workbook::open(&path)?;
        let elapsed = started.elapsed();
        let cells = workbook.sheets().iter().try_fold(0usize, |count, sheet| {
            count
                .checked_add(sheet.cells().len())
                .ok_or("cell count overflow")
        })?;
        writeln!(
            std::io::stdout().lock(),
            "run {run}: {:.3} ms, {} sheets, {cells} cells",
            elapsed.as_secs_f64() * 1000.,
            workbook.sheets().len(),
        )?;
        std::hint::black_box(&workbook);
    }
    Ok(())
}
