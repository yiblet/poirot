//! Modern PCRE2 functions retain operational failures and dynamic array geometry.
use super::modern::{description, member, number, output, shape_admission};
use crate::host::{CallHost, NumericPolicy, OperandDescription};
use crate::*;
use poirot_functions_text::regex::{self, Case, Extraction, Program};

/// Admission bridge for independently implemented text algorithms.
struct RegexHost<'a, 'b, 'c>(&'a mut CallHost<'b, 'c>);
impl regex::Host for RegexHost<'_, '_, '_> {
    /// Preserve the evaluator's spreadsheet/operational failure distinction.
    type Failure = FunctionFailure;
    fn execution_limits(&self) -> regex::ExecutionLimits {
        self.0.regex_settings()
    }
    fn work(&mut self, units: usize) -> FunctionResult<()> {
        self.0.charge_work(units as u64)
    }
    fn text(&mut self, bytes: usize) -> FunctionResult<()> {
        self.0.check_text(bytes)
    }
    fn values(&mut self, count: usize) -> FunctionResult<()> {
        self.0.check_values(count)
    }
}
fn failure(host: &CallHost<'_, '_>, failure: regex::Failure<FunctionFailure>) -> FunctionFailure {
    match failure {
        regex::Failure::Pattern | regex::Failure::TextLimit => CellError::Value.into(),
        regex::Failure::NoMatch => CellError::NA.into(),
        regex::Failure::Host(failure) => failure,
        regex::Failure::Unsupported(detail) => {
            host.unsupported(CapabilityId::FunctionAdapter, detail)
        }
        regex::Failure::Resource => host.allocation_failure().into(),
        regex::Failure::SizeOverflow => host.size_overflow().into(),
        regex::Failure::InvalidState => host.unsupported(
            CapabilityId::FunctionAdapter,
            "PCRE2 capture geometry did not satisfy the checked adapter state",
        ),
        // PCRE2 public ABI: NOMEMORY=-48, HEAP_FAILED=121. The verified
        // pcre2-sys revision/source is recorded beside compile_failure in the domain crate.
        regex::Failure::Native(code) => host.resource_failure(
            if code == -48 || code == 121 { crate::ResourceFailureKind::Allocation } else { crate::ResourceFailureKind::BackendGuard },
            format!("PCRE2 native execution guard or representation failure ({code}); native consumption unavailable"),
        ).into(),
    }
}
fn text(host: &mut CallHost<'_, '_>, value: &CellValue) -> FunctionResult<String> {
    if let Some(text) = value.text_ref() {
        return host.project_text(text);
    }
    match value {
        Error(error) => Err((*error).into()),
        Number(value) => host.coercev2_number_to_text(*value, NumericPolicy::PinnedPoi),
        _ => host.coercev2_scalar_text(value),
    }
}
fn option(
    host: &mut CallHost<'_, '_>,
    shapes: &[OperandDescription],
    index: usize,
    row: usize,
    column: usize,
) -> FunctionResult<f64> {
    match shapes.get(index) {
        None => Ok(0.),
        Some(shape) => {
            let operand = member(host, index, *shape, row, column)?;
            number(host, operand, 0.)
        }
    }
}
fn case(value: f64) -> FunctionResult<Case> {
    match value {
        0. => Ok(Case::Sensitive),
        1. => Ok(Case::Insensitive),
        _ => Err(CellError::Value.into()),
    }
}
fn mode(value: f64) -> FunctionResult<Extraction> {
    match value {
        0. => Ok(Extraction::First),
        1. => Ok(Extraction::All),
        2. => Ok(Extraction::Groups),
        _ => Err(CellError::Value.into()),
    }
}
fn scalar_member(
    host: &mut CallHost<'_, '_>,
    name: TextFunction,
    shapes: &[OperandDescription],
    row: usize,
    column: usize,
) -> FunctionResult<Vec<CellValue>> {
    let source = member(
        host,
        0,
        *shapes.first().ok_or(CellError::Value)?,
        row,
        column,
    )?;
    let pattern = member(
        host,
        1,
        *shapes.get(1).ok_or(CellError::Value)?,
        row,
        column,
    )?;
    let source = text(host, &source.value)?;
    let pattern = text(host, &pattern.value)?;
    let (extraction, occurrence, replacement, sensitivity) = match name {
        TextFunction::RegexTest => (
            Extraction::First,
            0,
            None,
            case(option(host, shapes, 2, row, column)?)?,
        ),
        TextFunction::RegexExtract => (
            mode(option(host, shapes, 2, row, column)?)?,
            0,
            None,
            case(option(host, shapes, 3, row, column)?)?,
        ),
        TextFunction::RegexReplace => {
            let operand = member(
                host,
                2,
                *shapes.get(2).ok_or(CellError::Value)?,
                row,
                column,
            )?;
            let replacement = text(host, &operand.value)?;
            let occurrence = option(host, shapes, 3, row, column)?;
            if !occurrence.is_finite() || occurrence.abs() >= i64::MAX as f64 {
                return Err(CellError::Value.into());
            }
            (
                Extraction::First,
                occurrence.trunc() as i64,
                Some(replacement),
                case(option(host, shapes, 4, row, column)?)?,
            )
        }
        _ => return Err(CellError::Value.into()),
    };
    let program = Program::compile(&pattern, sensitivity, &mut RegexHost(host))
        .map_err(|e| failure(host, e))?;
    match name {
        TextFunction::RegexTest => {
            let value = program
                .test(&source, &mut RegexHost(host))
                .map_err(|e| failure(host, e))?;
            host.check_values(1)?;
            Ok(vec![Boolean(value)])
        }
        TextFunction::RegexReplace => {
            let value = program
                .replace(
                    &source,
                    replacement.as_deref().ok_or(CellError::Value)?,
                    occurrence,
                    &mut RegexHost(host),
                )
                .map_err(|e| failure(host, e))?;
            host.check_values(1)?;
            Ok(vec![Text(value)])
        }
        TextFunction::RegexExtract => {
            let values = program
                .extract(&source, extraction, &mut RegexHost(host))
                .map_err(|e| failure(host, e))?;
            host.check_values(values.len())?;
            Ok(values.into_iter().map(Text).collect())
        }
        _ => Err(CellError::Value.into()),
    }
}
fn evaluate(
    host: &mut CallHost<'_, '_>,
    name: TextFunction,
    length: usize,
    address: CellAddress,
) -> FunctionResult<Value> {
    host.check_values(length)?;
    let mut shapes = Vec::new();
    shapes
        .try_reserve_exact(length)
        .map_err(|_| host.allocation_failure())?;
    let mut rows = 1;
    let mut columns = 1;
    for index in 0..length {
        let shape = description(host, index)?;
        rows = rows.max(shape.rows);
        columns = columns.max(shape.columns);
        // SAFETY: The local length reservation covers this length-iteration loop.
        shapes.push(shape);
    }
    shape_admission(host, rows, columns)?;
    // Variable-length extracted results cannot become arrays of nested arrays.
    // First-match extraction, matching and replacement broadcast singleton axes.
    let count = rows
        .checked_mul(columns)
        .ok_or_else(|| host.size_overflow())?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| host.allocation_failure())?;
    for row in 0..rows {
        for column in 0..columns {
            let result = scalar_member(host, name, &shapes, row, column);
            let mut selected = match result {
                Ok(values) => values,
                Err(FunctionFailure::Spreadsheet(error)) => vec![Error(error)],
                Err(error) => return Err(error),
            };
            if count == 1 {
                let length = selected.len();
                // Excel returns all matches down a column, capture groups across a row.
                let groups =
                    name == TextFunction::RegexExtract && option(host, &shapes, 2, 0, 0)? == 2.;
                return output(
                    host,
                    selected,
                    if groups { 1 } else { length },
                    if groups { length } else { 1 },
                    address,
                );
            }
            if selected.len() != 1 {
                return Err(CellError::Calc.into());
            }
            // SAFETY: count was checked/reserved locally and bounds these nested loops.
            values.push(selected.pop().ok_or(CellError::Value)?);
        }
    }
    output(host, values, rows, columns, address)
}
/// Evaluate modern regex functions under the documentation profile.
pub(crate) fn execute(
    name: TextFunction,
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let length = args.len();
    let valid = match name {
        TextFunction::RegexTest => (2..=3).contains(&length),
        TextFunction::RegexExtract => (2..=4).contains(&length),
        TextFunction::RegexReplace => (3..=5).contains(&length),
        _ => false,
    };
    if !valid {
        return Ok(err(CellError::Value));
    }
    let mut host = CallHost::new_array(session, sheet, address, args, length);
    match evaluate(&mut host, name, length, address) {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}
