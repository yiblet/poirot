//! Compare Poirot and rxls loading, then exercise retained-package editing.
//! rxls 0.1.4 is the MIT-licensed backend; this compares its raw model with Poirot projection.
//! Run `cargo run --release --example rxls_trial -- INPUT [NEW_OUTPUT]`.
//! Use `INPUT --compare-only` to inspect differences without timing or editing.
//! Inputs are read into memory before timing; timings exclude model destruction.
//! Output uses create-new semantics and never overwrites an existing file.
//! No upstream code or fixtures are reused. Dependency provenance:
//! https://github.com/HyunjoJung/rxls/tree/c708e80d9b79fcfa0d172ec4ca8786885cdfc202
use poirot::Workbook;
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
    time::Instant,
};

/// Report trial results while propagating output failures to the caller.
macro_rules! report {
    ($($arg:tt)*) => {
        writeln!(std::io::stdout().lock(), $($arg)*)?
    };
}

fn normalize_line_ends(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn increment(
    counts: &mut BTreeMap<&'static str, usize>,
    key: &'static str,
) -> Result<(), Box<dyn std::error::Error>> {
    let count = counts.entry(key).or_default();
    *count = count.checked_add(1).ok_or("difference count overflow")?;
    Ok(())
}

fn rxls_cell_count(workbook: &rxls::Workbook) -> Result<usize, Box<dyn std::error::Error>> {
    let mut count = 0usize;
    for sheet in &workbook.sheets {
        for _ in sheet.cells() {
            count = count.checked_add(1).ok_or("cell count overflow")?;
        }
    }
    Ok(count)
}

fn compare(bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let poirot = Workbook::from_bytes(bytes)?;
    let rxls = rxls::Workbook::open(bytes)?;
    let mut differences = BTreeMap::<&str, usize>::new();
    for sheet in poirot.sheets() {
        let Some(other) = rxls.sheet_by_name(sheet.name()) else {
            increment(&mut differences, "missing sheet")?;
            continue;
        };
        let cells: BTreeMap<_, _> = other
            .cells()
            .map(|(r, c, v)| ((r, u32::from(c)), v))
            .collect();
        for (at, cell) in sheet.cells() {
            let Some(&other) = cells.get(&(at.row, at.col)) else {
                let kind = if cell.value == poirot::CellValue::Blank && cell.formula.is_none() {
                    "omitted blank"
                } else {
                    "missing nonblank or formula"
                };
                increment(&mut differences, kind)?;
                continue;
            };
            let (formula, value) = match other {
                rxls::Cell::Formula { formula, cached } => {
                    (Some(formula.as_str()), cached.as_ref())
                }
                other => (None, other),
            };
            if cell.formula.as_deref() != formula {
                increment(&mut differences, "formula text differs")?;
                if cell.formula.as_deref() == formula.map(normalize_line_ends).as_deref() {
                    increment(&mut differences, "formula difference is line endings only")?;
                }
            }
            let equal = match (&cell.value, value) {
                (poirot::CellValue::Number(a), rxls::Cell::Number(b) | rxls::Cell::Date(b)) => {
                    a.to_bits() == b.to_bits()
                }
                (poirot::CellValue::Boolean(a), rxls::Cell::Bool(b)) => a == b,
                (poirot::CellValue::Error(a), rxls::Cell::Error(b)) => a.to_string() == *b,
                (a, rxls::Cell::Text(b)) => {
                    a.text_ref().is_some_and(|a| a.units().eq(b.encode_utf16()))
                }
                _ => false,
            };
            if !equal {
                increment(&mut differences, "value or cache differs")?;
                if let rxls::Cell::Text(value) = value
                    && cell
                        .value
                        .text_ref()
                        .is_some_and(|a| a.units().eq(normalize_line_ends(value).encode_utf16()))
                {
                    increment(&mut differences, "value difference is line endings only")?;
                }
            }
        }
    }
    report!("differential comparison against Poirot (not an independent oracle): {differences:?}");
    Ok(())
}

fn parts(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, Box<dyn std::error::Error>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut parts = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name()?.into_owned();
        let mut contents = Vec::new();
        entry.read_to_end(&mut contents)?;
        parts.insert(name, contents);
    }
    Ok(parts)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = args.next().ok_or("usage: rxls_trial INPUT [NEW_OUTPUT]")?;
    let output = args.next();
    if args.next().is_some() {
        return Err("usage: rxls_trial INPUT [NEW_OUTPUT]".into());
    }
    let bytes = std::fs::read(input)?;
    compare(&bytes)?;
    if output.as_deref() == Some(std::ffi::OsStr::new("--compare-only")) {
        return Ok(());
    }
    for run in 1..=5 {
        let started = Instant::now();
        let workbook = Workbook::from_bytes(&bytes)?;
        let elapsed = started.elapsed();
        let cells = workbook.sheets().iter().try_fold(0usize, |count, sheet| {
            count
                .checked_add(sheet.cells().len())
                .ok_or("cell count overflow")
        })?;
        report!(
            "Poirot run {run}: {:.3} ms, {} sheets, {cells} cells",
            elapsed.as_secs_f64() * 1000.,
            workbook.sheets().len()
        );
        std::hint::black_box(&workbook);
        let started = Instant::now();
        let workbook = rxls::Workbook::open(&bytes)?;
        let elapsed = started.elapsed();
        let cells = rxls_cell_count(&workbook)?;
        report!(
            "rxls read run {run}: {:.3} ms, {} sheets, {cells} cells",
            elapsed.as_secs_f64() * 1000.,
            workbook.sheets.len()
        );
        std::hint::black_box(&workbook);
    }
    let started = Instant::now();
    let mut spreadsheet = rxls::Spreadsheet::open(&bytes)?;
    report!(
        "rxls retained open: {:.3} ms, {:?}",
        started.elapsed().as_secs_f64() * 1000.,
        spreadsheet.edit_capability()
    );
    let original = parts(&bytes)?;
    let started = Instant::now();
    let unchanged = spreadsheet.save()?;
    report!(
        "rxls no-op save: {:.3} ms",
        started.elapsed().as_secs_f64() * 1000.
    );
    report!(
        "no-op part contents equal: {}",
        original == parts(&unchanged)?
    );
    let mut literal = None;
    let mut formula = None;
    for sheet in &spreadsheet.workbook().sheets {
        for (row, col, cell) in sheet.cells() {
            if literal.is_none() && matches!(cell, rxls::Cell::Number(_)) {
                literal = Some((sheet.name.clone(), row, col));
            }
            if formula.is_none() && matches!(cell, rxls::Cell::Formula { .. }) {
                formula = Some((sheet.name.clone(), row, col));
            }
            if literal.is_some() && formula.is_some() {
                break;
            }
        }
        if literal.is_some() && formula.is_some() {
            break;
        }
    }
    let (literal_sheet, literal_row, literal_col) = literal.ok_or("no numeric literal found")?;
    let (formula_sheet, formula_row, formula_col) = formula.ok_or("no formula found")?;
    let started = Instant::now();
    spreadsheet.set_cell_value(
        &literal_sheet,
        literal_row,
        literal_col,
        rxls::Cell::Number(123.5),
    )?;
    report!(
        "rxls literal edit: {:.3} ms",
        started.elapsed().as_secs_f64() * 1000.
    );
    let started = Instant::now();
    spreadsheet.set_cell_formula(
        &formula_sheet,
        formula_row,
        formula_col,
        "1+2",
        rxls::Cell::Number(3.),
    )?;
    report!(
        "rxls formula edit: {:.3} ms",
        started.elapsed().as_secs_f64() * 1000.
    );
    let started = Instant::now();
    let edited = spreadsheet.save()?;
    report!(
        "rxls edited save: {:.3} ms",
        started.elapsed().as_secs_f64() * 1000.
    );
    let saved = parts(&edited)?;
    let mut preserved = 0usize;
    for (name, source) in &original {
        match saved.get(name) {
            Some(target) if target == source => {
                preserved = preserved.checked_add(1).ok_or("part count overflow")?
            }
            Some(_) => report!("changed part: {name}"),
            None => report!("removed part: {name}"),
        }
    }
    for name in saved.keys().filter(|name| !original.contains_key(*name)) {
        report!("added part: {name}");
    }
    report!(
        "preserved {preserved}/{} original part contents",
        original.len()
    );
    let reopened = rxls::Workbook::open(&edited)?;
    let literal = reopened
        .sheet_by_name(&literal_sheet)
        .and_then(|s| s.cell(literal_row, literal_col));
    let formula = reopened
        .sheet_by_name(&formula_sheet)
        .and_then(|s| s.cell(formula_row, formula_col));
    if literal != Some(&rxls::Cell::Number(123.5))
        || formula
            != Some(&rxls::Cell::Formula {
                formula: "1+2".into(),
                cached: Box::new(rxls::Cell::Number(3.)),
            })
    {
        return Err("edited values did not survive reopening".into());
    }
    report!("edited value, formula, and cache verified after reopening");
    if let Some(output) = output {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?
            .write_all(&edited)?;
    }
    Ok(())
}
