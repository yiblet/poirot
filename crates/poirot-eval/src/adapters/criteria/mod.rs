//! Indexed criteria aggregation following pinned POI Countif, Sumif and Baseifs.
use crate::criteria_adapter::{Function, cell, error};
use crate::host::{CallHost, MissingPolicy, NumericPolicy, OperandKind};
use crate::*;
use poirot_functions_criteria::{Cell, Criterion};

/// Dispatch criteria aggregation with lazy, metered workbook reads.
pub(crate) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: Function,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let mut host = CallHost::new(session, sheet, address, arguments);
    match evaluate(&mut host, function) {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(e)) => Ok(err(e)),
        Err(FunctionFailure::Operational(e)) => Err(e),
    }
}
fn range(host: &mut CallHost<'_, '_>, index: usize) -> FunctionResult<(usize, usize)> {
    if !matches!(
        host.kind(index)?,
        OperandKind::Reference | OperandKind::Array
    ) {
        return Err(VALUE_ERROR.into());
    }
    host.shape(index)
}
/// Admit text payloads and reject error kinds unsupported by criteria helpers.
pub(super) fn preflight(host: &mut CallHost<'_, '_>, values: &[&CellValue]) -> FunctionResult<()> {
    let bytes = values
        .iter()
        .map(|v| v.text_storage_bytes())
        .fold(0usize, usize::saturating_add);
    host.check_text(bytes.saturating_mul(3))?;
    host.check_values(bytes.saturating_mul(32).saturating_add(values.len()))?;
    host.charge_work(
        (bytes as u64)
            .saturating_mul(bytes as u64)
            .saturating_add(1),
    )?;
    if values
        .iter()
        .any(|v| matches!(v, Error(GettingData | Spill | Calc)))
    {
        return Err(host.unsupported(CapabilityId::FunctionAdapter, "criteria domain error code"));
    }
    Ok(())
}
fn evaluate(host: &mut CallHost<'_, '_>, function: Function) -> FunctionResult<Value> {
    use Function::*;
    if function == Subtotal {
        return subtotal(host);
    }
    let single = matches!(function, CountIf | SumIf | AverageIf);
    let count = matches!(function, CountIf | CountIfs);
    let n = host.len();
    if (single && (n < 2 || (function != AverageIf && n > if count { 2 } else { 3 })))
        || (!single
            && (n < if count { 2 } else { 3 } || !(n - usize::from(!count)).is_multiple_of(2)))
    {
        return Err(VALUE_ERROR.into());
    }
    let start = usize::from(!single && !count);
    let mut target = if !single && !count {
        range(host, 0)?;
        Some(0)
    } else {
        None
    };
    host.check_values(n)?;
    let mut ranges = Vec::new();
    let mut criteria = Vec::new();
    // SAFETY: The step is the nonzero literal 2.
    for i in (start..if single { 2 } else { n }).step_by(2) {
        let shape = range(host, i)?;
        ranges.push((i, shape));
        if single && !count {
            target = Some(if n == 3 {
                range(host, 2)?;
                2
            } else {
                0
            });
        }
        // Argument expression preparation does not read referenced criterion cells.
        let description = host.describe(i + 1)?;
        criteria.push((i + 1, description));
    }
    let shape = ranges.first().ok_or(VALUE_ERROR)?.1;
    if ranges.iter().any(|(_, s)| *s != shape)
        || (!single && target.is_some() && host.shape(target.ok_or(VALUE_ERROR)?)? != shape)
    {
        return Err(VALUE_ERROR.into());
    }
    let mut output_shape = (1, 1);
    for (_, d) in &criteria {
        if d.kind == OperandKind::Array && (d.rows, d.columns) != (1, 1) {
            if output_shape != (1, 1) && output_shape != (d.rows, d.columns) {
                return Err(VALUE_ERROR.into());
            }
            output_shape = (d.rows, d.columns);
        } else if d.kind == OperandKind::Reference && d.rows.saturating_mul(d.columns) > 1 {
            return Err(host.unsupported(
                CapabilityId::FunctionAdapter,
                "reference criterion array context is unavailable",
            ));
        }
    }
    let output_len = output_shape.0.checked_mul(output_shape.1).ok_or(Num)?;
    host.check_values(output_len)?;
    let mut output = Vec::with_capacity(output_len);
    for out in 0..output_len {
        let mut predicates = Vec::with_capacity(criteria.len());
        let mut criterion_error = None;
        for (index, d) in &criteria {
            let scalar = if d.kind == OperandKind::Array && d.rows * d.columns > 1 {
                host.read_cell(*index, out / output_shape.1, out % output_shape.1)?
            } else {
                host.scalar_data(*index, MissingPolicy::Blank)?
            };
            // Countif.createCriteriaPredicate preserves MissingArgEval and throws;
            // it must not be confused with a real BlankEval criterion.
            if scalar.missing {
                return Err(host.unsupported(
                    CapabilityId::FunctionAdapter,
                    "pinned MissingArgEval criteria predicate throws a source exception",
                ));
            }
            let v = scalar.value;
            preflight(host, &[&v])?;
            let predicate = Criterion::parse(&cell(host, &v)?);
            if (!single || function == AverageIf)
                && let Cell::Error(e) = predicate.value
                && criterion_error.is_none()
            {
                criterion_error = Some(error(e));
            }
            predicates.push(predicate);
        }
        let value = if let Some(e) = criterion_error {
            e
        } else {
            aggregate(host, function, &ranges, &predicates, target)?
        };
        output.push(value);
    }
    if output_shape == (1, 1) {
        Ok(output.pop().ok_or(VALUE_ERROR)?.into())
    } else {
        host.check_values(output_shape.0)?;
        if output_shape.1 == 0 {
            return Err(VALUE_ERROR.into());
        }
        Ok(Value::Array(crate::ArrayView::unknown(
            // SAFETY: The local guard rejects a zero chunk width.
            output.chunks(output_shape.1).map(|r| r.to_vec()).collect(),
        )))
    }
}
fn aggregate(
    host: &mut CallHost<'_, '_>,
    function: Function,
    ranges: &[(usize, (usize, usize))],
    criteria: &[Criterion],
    target: Option<usize>,
) -> FunctionResult<CellValue> {
    use Function::*;
    let (rows, columns) = ranges.first().ok_or(VALUE_ERROR)?.1;
    let (mut sum, mut count, mut extremum) = (0., 0usize, None::<f64>);
    for r in 0..rows {
        for c in 0..columns {
            let mut matched = true;
            for ((index, _), criterion) in ranges.iter().zip(criteria) {
                if function != AverageIf && matches!(criterion.value, Cell::Blank) {
                    matched = false;
                    break;
                }
                let v = host.read_cell(*index, r, c)?.value;
                let cv = match &criterion.value {
                    Cell::Text(s) => Text(s.clone()),
                    _ => Blank,
                };
                preflight(host, &[&v, &cv])?;
                if !criterion.matches(&cell(host, &v)?) {
                    matched = false;
                    break;
                }
            }
            // AverageIf reads the target even for nonmatches; other functions stay lazy.
            let value = if function == AverageIf {
                Some(
                    host.read_relative_cell(target.ok_or(VALUE_ERROR)?, r, c)?
                        .value,
                )
            } else {
                None
            };
            if !matched {
                continue;
            }
            if matches!(function, CountIf | CountIfs) {
                count += 1;
                continue;
            }
            let v = match value {
                Some(v) => v,
                None => {
                    host.read_relative_cell(target.ok_or(VALUE_ERROR)?, r, c)?
                        .value
                }
            };
            if let Error(e) = v
                && function != AverageIf
            {
                return Ok(Error(e));
            }
            if let Number(n) = v {
                sum += n;
                count += 1;
            }
            let numeric = if let Number(n) = v { n } else { 0. };
            extremum = Some(match extremum {
                None => numeric,
                Some(old) if function == MinIfs => old.min(numeric),
                Some(old) => old.max(numeric),
            });
        }
    }
    Ok(match function {
        CountIf | CountIfs => Number(count as f64),
        SumIf | SumIfs => finite(sum),
        AverageIf | AverageIfs if count == 0 => Error(Div0),
        AverageIf | AverageIfs => finite(sum / count as f64),
        MaxIfs | MinIfs => finite(extremum.unwrap_or(0.)),
        Subtotal => return Err(VALUE_ERROR.into()),
    })
}
fn subtotal(host: &mut CallHost<'_, '_>) -> FunctionResult<Value> {
    if host.len() < 2 {
        return Err(VALUE_ERROR.into());
    }
    // Subtotal uses OperandResolver.coerceValueToInt, including its i32
    // range check. MissingArgEval is outside its supported coercion types.
    let scalar = host.scalar_data(0, MissingPolicy::Blank)?;
    if scalar.missing {
        return Err(host.unsupported(
            CapabilityId::NumericConversion,
            "pinned MissingArgEval SUBTOTAL code coercion throws a source exception",
        ));
    }
    let code = match scalar.value {
        Text(text) => host.coercev2_number_text(&text, NumericPolicy::PinnedPoi)?,
        value => host.coercev2_scalar_number(&value, NumericPolicy::PinnedPoi)?,
    }
    .floor();
    if code.is_nan() || code > i32::MAX as f64 || code < i32::MIN as f64 {
        return Err(Num.into());
    }
    if !((1. ..=11.).contains(&code) || (101. ..=111.).contains(&code)) {
        return Err(VALUE_ERROR.into());
    }
    let mut cells = Vec::new();
    for index in 1..host.len() {
        if host.kind(index)? == OperandKind::Reference && !host.excel_builtins() {
            return Err(host.unsupported(
                CapabilityId::FunctionAdapter,
                "SUBTOTAL needs workbook visibility and nested subtotal metadata",
            ));
        }
        let len = host.collection_len(index)?;
        host.check_values(cells.len().saturating_add(len))?;
        for position in 0..len {
            let v = if host.excel_builtins() {
                let Some(value) = host.subtotal_cell(index, position, code > 100.)? else {
                    continue;
                };
                value
            } else {
                host.collection_cell(index, position)?
            };
            let value = if v.missing { Number(0.) } else { v.value };
            preflight(host, &[&value])?;
            cells.push((
                cell(host, &value)?,
                v.origin != crate::host::OperandOrigin::Direct,
            ));
        }
    }
    host.check_values(cells.len())?;
    let supplied: Vec<_> = cells
        .iter()
        .map(
            |(value, referenced)| poirot_functions_criteria::subtotal::SubtotalCell {
                value,
                hidden: false,
                nested_subtotal: false,
                referenced: *referenced,
            },
        )
        .collect();
    host.charge_work(cells.len() as u64)?;
    Ok(
        match poirot_functions_criteria::subtotal::subtotal(code as i32, &supplied) {
            Ok(n) => Number(n),
            Err(e) => error(e),
        }
        .into(),
    )
}
