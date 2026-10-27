//! Replay independent scenarios with persistent, reset, or fresh overlays.
//! Input/output contents stay private; stdout contains only timings, counts and hashes.
mod serde;
mod snapshot;

use poirot::calculation::{EvaluationContext, EvaluationTarget, Overlay};
use poirot::{CellValue, Workbook};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    error::Error,
    io::{BufRead, BufReader},
    time::{Duration, Instant, UNIX_EPOCH},
};

/// Fallible benchmark operation; errors abort rather than becoming timing samples.
type Result<T> = std::result::Result<T, Box<dyn Error>>;
/// Coordinate key independent of an evaluation's temporary state.
type Key = (usize, poirot::CellAddress);
/// How much state survives between independent request scenarios.
#[derive(Clone, Copy, Debug)]
enum Mode {
    Persistent,
    Reset,
    Fresh,
}
/// A prepared authored override, with legacy formula-cache input behavior explicit.
#[derive(Clone, PartialEq)]
enum Input {
    Literal(CellValue),
    Formula(String),
    SavedValue(CellValue),
}
/// One independent scenario, resolved before measured execution begins.
struct Scenario {
    inputs: BTreeMap<Key, Input>,
    outputs: Vec<EvaluationTarget>,
    context: EvaluationContext,
    expected: Option<Vec<CellValue>>,
}
/// Timing and correctness evidence for one request, containing no spreadsheet values.
struct Observation {
    setup_ms: f64,
    edit_ms: f64,
    evaluation_ms: f64,
    signature: String,
    errors: usize,
    outputs: usize,
    formulas: u64,
}

fn milliseconds(began: Instant) -> f64 {
    began.elapsed().as_secs_f64() * 1000.
}
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| format!("missing request field {key}").into())
}
fn object<'a>(value: &'a Value, key: &str) -> Result<&'a serde_json::Map<String, Value>> {
    field(value, key)?
        .as_object()
        .ok_or_else(|| format!("request field {key} must be an object").into())
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    field(value, key)?
        .as_str()
        .ok_or_else(|| format!("request field {key} must be a string").into())
}
fn target(key: Key) -> EvaluationTarget {
    EvaluationTarget {
        sheet: key.0,
        address: key.1,
    }
}
fn scenario(book: &Workbook, value: &Value) -> Result<Scenario> {
    let resolver = book.overlay().build()?;
    let mut inputs = BTreeMap::new();
    for mapping in object(value, "inputs")?.values() {
        let at = resolver.target(string(mapping, "cell")?)?;
        let sheet = book.sheet(at.sheet).ok_or("missing input sheet")?;
        // Restoring saved spill children requires source ownership edits outside this harness.
        if sheet.spill_owner(at.address).is_some() {
            return Err("benchmark input cannot replace a saved spill child".into());
        }
        let input = if let Some(formula) = mapping.get("formula").and_then(Value::as_str) {
            Input::Formula(formula.to_owned())
        } else if mapping.get("blank").and_then(Value::as_bool) == Some(true) {
            Input::Literal(CellValue::Blank)
        } else {
            let text = string(mapping, "string")?;
            let parsed = poirot_parsing::java_number::parse_java_number(text)
                .map(CellValue::Number)
                .unwrap_or_else(|| CellValue::Text(text.into()));
            if resolver
                .source_cell(at)
                .is_some_and(|cell| cell.formula.is_some())
            {
                Input::SavedValue(parsed)
            } else {
                Input::Literal(parsed)
            }
        };
        inputs.insert((at.sheet, at.address), input);
    }
    let outputs = object(value, "output_cells")?
        .values()
        .map(|cell| resolver.target(cell.as_str().unwrap_or("")))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let seconds = field(value, "clock_unix_seconds")?
        .as_f64()
        .ok_or("clock must be numeric")?;
    let context = EvaluationContext {
        as_of_time: UNIX_EPOCH
            .checked_add(Duration::try_from_secs_f64(seconds)?)
            .ok_or("clock exceeds supported range")?,
        randomness_seed: 0,
        ..Default::default()
    };
    Ok(Scenario {
        inputs,
        outputs,
        context,
        expected: None,
    })
}
/// Apply an authored override while preserving baseline formula-cache semantics.
fn apply<W: std::borrow::Borrow<poirot::model::Workbook>>(
    overlay: &mut Overlay<'_, W>,
    book: &Workbook,
    key: Key,
    input: &Input,
) -> Result<()> {
    match input {
        Input::Literal(value) => overlay.set_value(target(key), value.clone())?,
        Input::Formula(formula) => overlay.set_formula(target(key), formula.clone())?,
        Input::SavedValue(value) => {
            let at = target(key);
            let original = book
                .sheet(at.sheet)
                .and_then(|sheet| sheet.source_cell(at.address))
                .and_then(|cell| cell.formula.as_deref())
                .ok_or("saved-value input has no baseline formula")?;
            if overlay
                .source_cell(at)
                .and_then(|cell| cell.formula.as_deref())
                != Some(original)
            {
                overlay.set_formula(at, original)?;
            }
            overlay.set_cached_value(at, value.clone())?;
        }
    }
    Ok(())
}
/// Restore one omitted input to its original source content.
fn restore<W: std::borrow::Borrow<poirot::model::Workbook>>(
    overlay: &mut Overlay<'_, W>,
    book: &Workbook,
    key: Key,
) -> Result<()> {
    let at = target(key);
    if let Some(cell) = book
        .sheet(at.sheet)
        .and_then(|sheet| sheet.source_cell(at.address))
    {
        if let Some(formula) = &cell.formula {
            overlay.set_formula(at, formula.clone())?;
            overlay.set_cached_value(at, cell.value.clone())?;
        } else {
            overlay.set_value(at, cell.value.clone())?;
        }
        // Input writes preserve authored style, so restoring content needs no style edit.
    } else {
        overlay.remove_cell(at)?;
    }
    Ok(())
}
fn signature(values: &[poirot::calculation::CellEvaluation]) -> Result<String> {
    let encoded = values
        .iter()
        .map(|cell| match &cell.value {
            CellValue::Blank => json!(["blank"]),
            CellValue::Boolean(v) => json!(["boolean", v]),
            CellValue::Number(v) => json!(["number", format!("{:016x}", v.to_bits())]),
            CellValue::Error(error) => json!(["error", error.to_string()]),
            value => json!([
                "text",
                value
                    .text_ref()
                    .map(|text| text.units().collect::<Vec<_>>())
            ]),
        })
        .collect::<Vec<_>>();
    Ok(Sha256::digest(serde_json::to_vec(&encoded)?)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
fn replay(book: &Workbook, scenarios: &[Scenario], mode: Mode) -> Result<Vec<Observation>> {
    let initial_setup = Instant::now();
    let mut overlay = book.overlay().build()?;
    let initial_setup_ms = milliseconds(initial_setup);
    let mut previous = BTreeMap::new();
    let mut observations = Vec::new();
    for request in scenarios {
        let began = Instant::now();
        if !observations.is_empty() {
            match mode {
                Mode::Persistent => {}
                Mode::Reset => overlay.discard_edits(),
                Mode::Fresh => overlay = book.overlay().build()?,
            }
        }
        let setup_ms = milliseconds(began)
            + if observations.is_empty() {
                initial_setup_ms
            } else {
                0.
            };
        let began = Instant::now();
        if matches!(mode, Mode::Persistent) {
            for &key in previous
                .keys()
                .filter(|key| !request.inputs.contains_key(key))
            {
                restore(&mut overlay, book, key)?;
            }
        }
        for (&key, input) in &request.inputs {
            if !matches!(mode, Mode::Persistent) || previous.get(&key) != Some(input) {
                apply(&mut overlay, book, key, input)?;
            }
        }
        previous = request.inputs.clone();
        let edit_ms = milliseconds(began);
        let began = Instant::now();
        let mut cycle = overlay.evaluation_with_profiling(&request.context, true)?;
        let values = cycle
            .outputs(&request.outputs)
            .map_err(|error| format!("request index {}: {error}", observations.len()))?;
        let evaluation_ms = milliseconds(began);
        let report = cycle.finish();
        report.result?;
        let formulas = report
            .execution
            .ok_or("profiling cycle returned no execution counters")?
            .formula_executions;
        if let Some(expected) = &request.expected
            && !values.iter().map(|cell| &cell.value).eq(expected.iter())
        {
            return Err("synthetic outputs disagree with independent mathematical control".into());
        }
        observations.push(Observation {
            setup_ms,
            edit_ms,
            evaluation_ms,
            formulas,
            signature: signature(&values)?,
            errors: values
                .iter()
                .filter(|cell| matches!(cell.value, CellValue::Error(_)))
                .count(),
            outputs: values.len(),
        });
    }
    Ok(observations)
}
fn load_requests(book: &Workbook, path: &str, limit: usize) -> Result<Vec<Scenario>> {
    BufReader::new(std::fs::File::open(path)?)
        .lines()
        .take(limit)
        .map(|line| scenario(book, &serde_json::from_str::<Value>(&line?)?))
        .collect()
}
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("snapshot") {
        return snapshot::run(&args);
    }
    if args.first().map(String::as_str) == Some("serde") {
        return serde::run(&args);
    }
    let mode = match args.first().map(String::as_str) {
        Some("persistent") => Mode::Persistent,
        Some("reset") => Mode::Reset,
        Some("fresh") => Mode::Fresh,
        _ => return Err("usage: replay persistent|reset|fresh|snapshot workbook.xlsx requests.jsonl [limit] OR serde workbook.xlsx [repetitions] OR MODE --synthetic CASE [limit]".into()),
    };
    let input = args.get(1).ok_or("missing workbook or --synthetic")?;
    let requests = args
        .get(2)
        .ok_or("missing requests file or synthetic case")?;
    let limit = args.get(3).map(|v| v.parse()).transpose()?.unwrap_or(100);
    let load = Instant::now();
    let (book, scenarios, load_ms, requests_ms) = if input == "--synthetic" {
        let (book, scenarios) = synthetic(requests, limit)?;
        (book, scenarios, 0., milliseconds(load))
    } else {
        let bytes = std::fs::read(input)?;
        let options = poirot::files::ReadOptions {
            max_input_bytes: u64::MAX,
            max_zip_entry_bytes: u64::MAX,
            max_zip_total_bytes: u64::MAX,
            max_zip_entries: usize::MAX,
            ..Default::default()
        };
        let book = Workbook::load().options(options).from_bytes(&bytes)?;
        let load_ms = milliseconds(load);
        let began = Instant::now();
        let scenarios = load_requests(&book, requests, limit)?;
        (book, scenarios, load_ms, milliseconds(began))
    };
    let observations = replay(&book, &scenarios, mode)?;
    let report = observations.iter().map(|r| json!({
        "setup_ms":r.setup_ms,"edit_ms":r.edit_ms,"evaluation_ms":r.evaluation_ms,
        "signature":r.signature,"errors":r.errors,"outputs":r.outputs,"formula_executions":r.formulas,
    })).collect::<Vec<_>>();
    println!(
        "{}",
        json!({"load_ms":load_ms,"requests_ms":requests_ms,"requests":report})
    );
    Ok(())
}

/// Independent arithmetic controls for unchanged diamonds, lazy switches and spill edits.
fn synthetic(name: &str, limit: usize) -> Result<(Workbook, Vec<Scenario>)> {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S")?;
    let mut requests = Vec::new();
    match name {
        "diamond" => {
            sheet.set_value("A1".parse()?, 1.1)?;
            sheet.set_value("A2".parse()?, 2.1)?;
            sheet.set_formula("B1".parse()?, "ROUND(A1,0)")?;
            sheet.set_formula("B2".parse()?, "ROUND(A2,0)")?;
            for row in 1..=500 {
                sheet.set_value(format!("F{row}").parse()?, f64::from(row))?;
                sheet.set_formula(
                    format!("D{row}").parse()?,
                    format!("SUMPRODUCT(($F$1:$F$500+B1)*B2)+{row}"),
                )?;
            }
            for index in 0..limit {
                // SAFETY: literal nonzero divisors bound the deterministic phase.
                let delta = (index % 3) as f64 / 10.;
                requests.push((json!({"inputs":{"a":{"cell":"S!A1","string":(1.1+delta).to_string()},"b":{"cell":"S!A2","string":(2.1+delta).to_string()}},"output_cells":{"out":"S!D500"},"clock_unix_seconds":0}), vec![CellValue::Number(252000.)]));
            }
        }
        "branches" => {
            sheet.set_value("A1".parse()?, 1.)?;
            sheet.set_formula("B1".parse()?, "A1>0")?;
            sheet.set_formula("B2".parse()?, "IF(A1>0,10,1/0)")?;
            sheet.set_formula("C1".parse()?, "IF(B1,B2,20)")?;
            for index in 0..limit {
                // SAFETY: literal nonzero divisor defines alternating branches.
                let positive = index % 2 == 0;
                requests.push((json!({"inputs":{"a":{"cell":"S!A1","string":if positive {"1"} else {"-1"}}},"output_cells":{"out":"S!C1"},"clock_unix_seconds":0}),vec![CellValue::Number(if positive {10.} else {20.})]));
            }
        }
        "spill" => {
            sheet.set_value("F1".parse()?, 2.1)?;
            sheet.set_value("F2".parse()?, 10.1)?;
            sheet.set_formula("G1".parse()?, "ROUND(F1,0)")?;
            sheet.set_formula("G2".parse()?, "ROUND(F2,0)")?;
            sheet.set_formula("D1".parse()?, "A2*2")?;
            for index in 0..limit {
                // SAFETY: literal nonzero divisor defines three independent scenarios.
                let phase = index % 3;
                let mut inputs = serde_json::Map::new();
                inputs.insert(
                    "f1".into(),
                    json!({"cell":"S!F1","string":if phase == 2 {"2.2"} else {"2.1"}}),
                );
                inputs.insert(
                    "f2".into(),
                    json!({"cell":"S!F2","string":if phase == 2 {"10.2"} else {"10.1"}}),
                );
                if phase != 0 {
                    inputs.insert(
                        "anchor".into(),
                        json!({"cell":"S!A1","formula":"SEQUENCE(G1,1,1,G2)"}),
                    );
                }
                requests.push((
                    json!({"inputs":inputs,"output_cells":{"out":"S!D1"},"clock_unix_seconds":0}),
                    vec![CellValue::Number(if phase == 0 { 0. } else { 22. })],
                ));
            }
        }
        _ => return Err("unknown synthetic case".into()),
    }
    let scenarios = requests
        .into_iter()
        .map(|(value, expected)| {
            let mut request = scenario(&book, &value)?;
            request.expected = Some(expected);
            Ok(request)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((book, scenarios))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_controls_agree_for_persistent_reset_and_fresh_runs() {
        for name in ["diamond", "branches", "spill"] {
            let (book, scenarios) = synthetic(name, 7).unwrap();
            let fresh = replay(&book, &scenarios, Mode::Fresh).unwrap();
            for mode in [Mode::Persistent, Mode::Reset] {
                let actual = replay(&book, &scenarios, mode)
                    .unwrap_or_else(|error| panic!("{name} {mode:?}: {error}"));
                assert!(
                    actual
                        .iter()
                        .zip(&fresh)
                        .all(|(a, b)| a.signature == b.signature)
                );
                assert!(actual.iter().all(|r| r.errors == 0));
            }
        }
    }

    #[test]
    fn omitted_inputs_restore_baseline_values_formulas_and_missing_cells() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet.set_value("A1".parse().unwrap(), 3.).unwrap();
        sheet.set_formula("A2".parse().unwrap(), "A1*2").unwrap();
        sheet
            .set_formula("D1".parse().unwrap(), "A1+A2+A3")
            .unwrap();
        let edited = json!({"inputs":{"a":{"cell":"S!A1","string":"5"},"b":{"cell":"S!A2","formula":"A1*3"},"c":{"cell":"S!A3","string":"7"}},"output_cells":{"out":"S!D1"},"clock_unix_seconds":0});
        let empty = json!({"inputs":{},"output_cells":{"out":"S!D1"},"clock_unix_seconds":0});
        let mut first = scenario(&book, &edited).unwrap();
        first.expected = Some(vec![27.0.into()]);
        let mut second = scenario(&book, &empty).unwrap();
        second.expected = Some(vec![9.0.into()]);
        let scenarios = [first, second];
        let fresh = replay(&book, &scenarios, Mode::Fresh).unwrap();
        let retained = replay(&book, &scenarios, Mode::Persistent).unwrap();
        assert!(
            fresh
                .iter()
                .zip(retained)
                .all(|(a, b)| a.signature == b.signature)
        );
    }
}
