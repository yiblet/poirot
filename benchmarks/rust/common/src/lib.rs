//! Shared workload protocol, timing boundaries, and correctness gates for speed tests.
use serde::{Deserialize, Serialize};
use std::{io, time::Instant};

/// Operational or correctness failure, never a timing result.
pub type Error = Box<dyn std::error::Error>;

/// Authored numeric literal, tagged text, or formula shared by every engine.
#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum Input {
    /// Finite numeric input.
    Number(f64),
    /// Tagged literal text, available only under schema2.
    Text {
        /// Exact stored text, without formula parsing.
        text: String,
    },
    /// Formula including its leading equals sign.
    Formula(String),
}

/// Zero-based workbook coordinates for authored cells and requested outputs.
#[derive(Clone, Copy, Deserialize)]
pub struct Address {
    /// Zero-based sheet, defaulting to the original single sheet.
    #[serde(default)]
    pub sheet: usize,
    /// Zero-based row.
    pub row: u32,
    /// Zero-based column.
    pub col: u32,
}

/// One cell populated before a benchmark engine's initial calculation.
/// The shared workload representation lets each engine receive the same literals and formulas.
#[derive(Deserialize)]
pub struct Cell {
    /// Cell location.
    #[serde(flatten)]
    pub address: Address,
    /// Literal or formula content.
    pub input: Input,
}

/// Changed input and independently derived expected outputs after the edit.
#[derive(Deserialize)]
pub struct Edit {
    /// Input cell location.
    #[serde(flatten)]
    pub address: Address,
    /// New numeric literal or formula source, using the same representation as cells.
    pub value: Input,
    /// Expected results in output order.
    pub expected: Vec<f64>,
}

/// Identical in-memory spreadsheet workload for all comparison engines.
#[derive(Deserialize)]
pub struct Case {
    /// Stable workload identifier.
    pub name: String,
    /// Number of sheets, named Sheet1 through SheetN.
    #[serde(default = "one_sheet")]
    pub sheet_count: usize,
    /// Source cells, including formulas.
    pub cells: Vec<Cell>,
    /// Outputs in observable order; spatial probes may request only part of a model.
    pub outputs: Vec<Address>,
    /// Expected initial outputs.
    pub expected: Vec<f64>,
    /// Ordered input changes applied after the initial calculation.
    pub edits: Vec<Edit>,
}

/// Default sheet count for the original single-sheet protocol.
fn one_sheet() -> usize {
    1
}

/// Run configuration decoded before any timing starts.
#[derive(Deserialize)]
pub struct Suite {
    /// Input protocol revision.
    pub schema_version: u32,
    /// Untimed fresh-model warmup rounds per workload.
    pub warmup: u32,
    /// Recorded fresh-model rounds per workload.
    pub samples: u32,
    /// Measure one unchanged output read after the initial calculation.
    #[serde(default)]
    pub unchanged_read: bool,
    /// Workloads generated independently of any engine.
    pub cases: Vec<Case>,
}

/// Adapter operations that include each engine's native calculation policy.
pub trait Engine {
    /// Calculate and collect requested numeric outputs.
    fn read(&mut self, outputs: &[Address]) -> Result<Vec<f64>, Error>;
    /// Collect unchanged outputs without explicitly requesting recalculation.
    fn read_unchanged(&mut self, outputs: &[Address]) -> Result<Vec<f64>, Error> {
        self.read(outputs)
    }
    /// Whether unchanged reads retain execution state in this workflow.
    fn retains_results(&self) -> bool {
        true
    }
    /// Change an input, including any automatic invalidation or recalculation.
    fn edit(&mut self, edit: &Edit) -> Result<(), Error>;
}

/// Native workflow boundary shared by every comparison adapter.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    /// Construct, calculate, and read a fresh model.
    #[serde(rename = "build_calculate_read")]
    Construction,
    /// Read previously calculated results without an edit.
    #[serde(rename = "unchanged_read")]
    Unchanged,
    /// Reevaluate unchanged inputs through a stateless direct API.
    #[serde(rename = "warm_evaluate_read")]
    WarmEvaluation,
    /// Edit, recalculate, and read an existing model.
    #[serde(rename = "edit_calculate_read")]
    Edit,
}

/// One validated elapsed time from an engine process.
#[derive(Serialize)]
struct Timing<'a> {
    /// Workload identifier.
    case: &'a str,
    /// Construction/calculation/read or edit/calculation/read boundary.
    phase: Phase,
    /// Recorded fresh-model round.
    sample: u32,
    /// Edit ordinal, absent for construction.
    edit: Option<usize>,
    /// Monotonic elapsed nanoseconds within the engine process.
    elapsed_ns: u128,
}

/// Complete successful adapter response; failures produce a nonzero exit instead.
#[derive(Serialize)]
struct Report<'a> {
    /// Output protocol revision.
    schema_version: u32,
    /// Library identifier.
    engine: &'a str,
    /// Exact library release or local package version.
    version: &'a str,
    /// Native calculation strategy.
    mode: &'a str,
    /// Raw validated timings, without warmups.
    timings: Vec<Timing<'a>>,
}

/// Reject wrong, nonnumeric, or nonfinite outputs before reporting measurements.
pub fn validate(actual: &[f64], expected: &[f64], case: &str) -> Result<(), Error> {
    if actual.len() != expected.len() || actual.is_empty() {
        return Err(format!("{case}: output count mismatch").into());
    }
    for (index, (&value, &reference)) in actual.iter().zip(expected).enumerate() {
        if !value.is_finite()
            || !reference.is_finite()
            || (value - reference).abs() > 1e-9 + 1e-10 * reference.abs()
        {
            return Err(
                format!("{case}: output {index}: expected {reference}, got {value}").into(),
            );
        }
    }
    Ok(())
}

/// Validate feature revision, source spellings and coordinates before measurement.
pub fn validate_suite(suite: &Suite) -> Result<(), Error> {
    if !matches!(suite.schema_version, 1 | 2) || suite.samples == 0 || suite.cases.is_empty() {
        return Err("unsupported or empty benchmark suite".into());
    }
    if suite.schema_version == 1 && suite.unchanged_read {
        return Err("unchanged_read requires schema2".into());
    }
    for case in &suite.cases {
        if case.sheet_count == 0 || (suite.schema_version == 1 && case.sheet_count != 1) {
            return Err("multiple sheets require schema2".into());
        }
        for at in case
            .cells
            .iter()
            .map(|c| &c.address)
            .chain(&case.outputs)
            .chain(case.edits.iter().map(|e| &e.address))
        {
            if at.sheet >= case.sheet_count || at.row >= 1_048_576 || at.col >= 16_384 {
                return Err("source or output coordinate outside workbook".into());
            }
        }
        for input in case
            .cells
            .iter()
            .map(|c| &c.input)
            .chain(case.edits.iter().map(|e| &e.value))
        {
            match input {
                Input::Number(value) if value.is_finite() => {}
                Input::Text { .. } if suite.schema_version == 2 => {}
                Input::Formula(source) if source.starts_with('=') && source.len() > 1 => {}
                _ => {
                    return Err("inputs must be finite numbers or formulas beginning with =".into());
                }
            }
        }
        if suite.schema_version == 1
            && case
                .edits
                .iter()
                .any(|e| matches!(e.value, Input::Formula(_)))
        {
            return Err("formula edits require schema2".into());
        }
    }
    Ok(())
}

/// Run fresh-model warmups and samples; decode and result validation are untimed.
pub fn run<E: Engine>(
    engine: &str,
    version: &str,
    mode: &str,
    build: impl Fn(&Case) -> Result<E, Error>,
) -> Result<(), Error> {
    let suite: Suite = serde_json::from_reader(io::stdin().lock())?;
    validate_suite(&suite)?;
    let rounds = suite
        .warmup
        .checked_add(suite.samples)
        .ok_or("round overflow")?;
    let mut timings = Vec::new();
    for case in &suite.cases {
        for round in 0..rounds {
            let start = Instant::now();
            let mut model = build(case)?;
            let actual = model.read(&case.outputs)?;
            let elapsed_ns = start.elapsed().as_nanos();
            validate(&actual, &case.expected, &case.name)?;
            if let Some(sample) = round.checked_sub(suite.warmup) {
                timings.push(Timing {
                    case: &case.name,
                    phase: Phase::Construction,
                    sample,
                    edit: None,
                    elapsed_ns,
                });
            }
            if suite.unchanged_read {
                let start = Instant::now();
                let actual = model.read_unchanged(&case.outputs)?;
                let elapsed_ns = start.elapsed().as_nanos();
                validate(&actual, &case.expected, &case.name)?;
                if let Some(sample) = round.checked_sub(suite.warmup) {
                    timings.push(Timing {
                        case: &case.name,
                        phase: if model.retains_results() {
                            Phase::Unchanged
                        } else {
                            Phase::WarmEvaluation
                        },
                        sample,
                        edit: None,
                        elapsed_ns,
                    });
                }
            }
            for (index, edit) in case.edits.iter().enumerate() {
                let start = Instant::now();
                model.edit(edit)?;
                let actual = model.read(&case.outputs)?;
                let elapsed_ns = start.elapsed().as_nanos();
                validate(&actual, &edit.expected, &case.name)?;
                if let Some(sample) = round.checked_sub(suite.warmup) {
                    timings.push(Timing {
                        case: &case.name,
                        phase: Phase::Edit,
                        sample,
                        edit: Some(index),
                        elapsed_ns,
                    });
                }
            }
        }
    }
    serde_json::to_writer(
        io::stdout().lock(),
        &Report {
            schema_version: suite.schema_version,
            engine,
            version,
            mode,
            timings,
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate;

    #[test]
    fn protocol_revision_and_input_validation_are_explicit() {
        let source = r#"{"schema_version":2,"samples":1,"warmup":0,"unchanged_read":true,
            "cases":[{"name":"literal","cells":[{"row":0,"col":0,"input":{"text":"=A1"}}],
            "outputs":[{"row":0,"col":0}],"expected":[3],
            "edits":[{"row":0,"col":0,"value":"=1+2","expected":[3]}]}]}"#;
        let mut suite: super::Suite = serde_json::from_str(source).unwrap();
        assert!(super::validate_suite(&suite).is_ok());
        suite.schema_version = 1;
        assert!(super::validate_suite(&suite).is_err());
        suite.unchanged_read = false;
        assert!(super::validate_suite(&suite).is_err());
        for invalid in [r#"{"text":"x","formula":"=1"}"#, "true", "null"] {
            assert!(serde_json::from_str::<super::Input>(invalid).is_err());
        }
        let mut suite: super::Suite = serde_json::from_str(source).unwrap();
        suite.cases[0].cells[0].address.sheet = 1;
        assert!(super::validate_suite(&suite).is_err());
        suite.cases[0].sheet_count = 2;
        assert!(super::validate_suite(&suite).is_ok());
    }

    #[test]
    fn correctness_gate_rejects_missing_incorrect_and_nonfinite_results() {
        assert!(validate(&[300.], &[12. * 25.], "energy").is_ok());
        assert!(validate(&[], &[], "empty").is_err());
        assert!(validate(&[300.], &[600.], "wrong").is_err());
        assert!(validate(&[f64::NAN], &[300.], "nan").is_err());
        assert!(validate(&[300.], &[f64::INFINITY], "infinite").is_err());
    }
}
