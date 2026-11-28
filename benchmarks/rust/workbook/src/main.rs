//! Measure native XLSX import and a first calculation over the original formulas.

use benchmark_common::{Address, Error, validate};
use poirot::calculation::{Evaluation, EvaluationTarget};
use poirot::model::TextRef;
use poirot::{CellAddress, CellValue, Workbook};
use serde::{Deserialize, Serialize};
use std::{io, time::Instant};

/// One native engine in the workbook comparison.
enum Engine {
    Poirot,
    IronCalc,
}

/// One formula coordinate and any independently recorded expectation.
#[derive(Deserialize)]
struct Probe {
    #[serde(flatten)]
    address: Address,
    expected: Option<Expected>,
}

/// Independent model result read from the workbook oracle.
#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum Expected {
    /// A cached spreadsheet number.
    Number(f64),
    /// A cached Excel logical value.
    Boolean(bool),
    /// A cached plain string, including an explicit empty result.
    Text(String),
}

impl Expected {
    /// Compare one typed evaluation result against its independent spreadsheet cache.
    fn check(&self, actual: &CellValue, coordinate: &str) -> Result<(), Error> {
        match self {
            Self::Number(expected) => {
                let actual = actual
                    .as_number()
                    .ok_or_else(|| format!("{coordinate}: expected a number, got {actual:?}"))?;
                validate(&[actual], &[*expected], coordinate)
            }
            Self::Boolean(expected) if actual.as_boolean() == Some(*expected) => Ok(()),
            Self::Text(expected) if actual.text_ref() == Some(TextRef::Utf8(expected)) => Ok(()),
            _ => Err(format!("{coordinate}: expected {self:?}, got {actual:?}").into()),
        }
    }
}

/// Frozen observer addresses and the number of fresh model repetitions.
#[derive(Deserialize)]
struct Request {
    probes: Vec<Probe>,
    samples: usize,
    warmup: usize,
}

/// Elapsed time for one fresh native import and calculation.
#[derive(Serialize)]
struct Sample {
    import_ns: u128,
    calculate_ns: u128,
}

/// Load a Poirot workbook, then calculate the supplied formula coordinates.
fn poirot(bytes: &[u8], probes: &[Probe]) -> Result<(Sample, Vec<CellValue>), Error> {
    let targets = probes
        .iter()
        .map(|probe| {
            Ok::<_, Error>(EvaluationTarget {
                sheet: probe.address.sheet,
                address: CellAddress::new(probe.address.row, probe.address.col)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let start = Instant::now();
    let book = Workbook::load().from_bytes(bytes)?;
    let mut evaluation = Evaluation::with_workbook(book).build()?;
    let import_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    let values = evaluation
        .outputs(&targets)?
        .into_iter()
        .map(|result| result.value)
        .collect();
    let calculate_ns = start.elapsed().as_nanos();
    Ok((
        Sample {
            import_ns,
            calculate_ns,
        },
        values,
    ))
}

/// Load an IronCalc workbook, then calculate the model and read the same coordinates.
fn ironcalc(bytes: &[u8], probes: &[Probe]) -> Result<(Sample, Vec<CellValue>), Error> {
    let coordinates = probes
        .iter()
        .map(|probe| {
            Ok::<_, Error>((
                u32::try_from(probe.address.sheet)?,
                i32::try_from(probe.address.row)?
                    .checked_add(1)
                    .ok_or("row overflow")?,
                i32::try_from(probe.address.col)?
                    .checked_add(1)
                    .ok_or("column overflow")?,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let start = Instant::now();
    let book = ironcalc::import::load_from_xlsx_bytes(bytes, "benchmark", "en", "UTC")?;
    let mut model = ironcalc_base::Model::from_workbook(book, "en")?;
    let import_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    model.evaluate();
    let values = coordinates
        .iter()
        .copied()
        .map(|(sheet, row, col)| {
            Ok::<_, Error>(match model.get_cell_value_by_index(sheet, row, col)? {
                ironcalc_base::cell::CellValue::Number(value) => CellValue::Number(value),
                ironcalc_base::cell::CellValue::String(value) => CellValue::Text(value),
                ironcalc_base::cell::CellValue::Boolean(value) => CellValue::Boolean(value),
                ironcalc_base::cell::CellValue::None => CellValue::Blank,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let calculate_ns = start.elapsed().as_nanos();
    // IronCalc exposes spreadsheet errors through the same string variant as
    // text. Check the stored type outside the timed read for text expectations.
    for (probe, (sheet, row, col)) in probes.iter().zip(coordinates) {
        if matches!(probe.expected, Some(Expected::Text(_)))
            && model.get_cell_type(sheet, row, col)? == ironcalc_base::types::CellType::ErrorValue
        {
            let at = probe.address;
            return Err(format!(
                "sheet {} row {} column {}: expected text, got a spreadsheet error",
                at.sheet, at.row, at.col
            )
            .into());
        }
    }
    Ok((
        Sample {
            import_ns,
            calculate_ns,
        },
        values,
    ))
}

/// Emit timings only after each sampled calculation passes the independent checks.
fn main() -> Result<(), Error> {
    let mut args = std::env::args_os().skip(1);
    let engine = match args.next().as_deref().and_then(std::ffi::OsStr::to_str) {
        Some("poirot") => Engine::Poirot,
        Some("ironcalc") => Engine::IronCalc,
        _ => {
            return Err(
                "usage: workbook-speed <poirot|ironcalc> <workbook.xlsx> [timings.json]".into(),
            );
        }
    };
    let path = args.next().ok_or("missing workbook path")?;
    let report = args.next();
    if args.next().is_some() {
        return Err("unexpected workbook arguments".into());
    }
    let bytes = std::fs::read(path)?;
    let request: Request = serde_json::from_reader(io::stdin().lock())?;
    if request.samples == 0 || !request.probes.iter().any(|probe| probe.expected.is_some()) {
        return Err("at least one sample and one independent expectation are required".into());
    }
    let rounds = request
        .samples
        .checked_add(request.warmup)
        .ok_or("sample count overflow")?;
    let mut timings = Vec::new();
    for sample in 0..rounds {
        let (timing, values) = match engine {
            Engine::Poirot => poirot(&bytes, &request.probes)?,
            Engine::IronCalc => ironcalc(&bytes, &request.probes)?,
        };
        if values.len() != request.probes.len() {
            return Err("calculation output count did not match the formula probes".into());
        }
        for (probe, actual) in request.probes.iter().zip(values) {
            if let Some(expected) = &probe.expected {
                let address = probe.address;
                let coordinate = format!(
                    "sheet {} row {} column {}",
                    address.sheet, address.row, address.col
                );
                expected.check(&actual, &coordinate)?;
            }
        }
        if sample >= request.warmup {
            timings.push(timing);
        }
    }
    if let Some(path) = report {
        serde_json::to_writer(std::fs::File::create(path)?, &timings)?;
    } else {
        serde_json::to_writer(io::stdout().lock(), &timings)?;
    }
    Ok(())
}
