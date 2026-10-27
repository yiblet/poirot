//! Shared lossless text coercion, integer selection, and output admission.
use super::broadcast::Broadcast;
use super::member_context::MemberContext;
use crate::eval::host::{ArgumentCoercion, CallHost, ScalarOperand, SelectionAccess};
use crate::eval::values::{ArrayView, FunctionError, FunctionResult, VALUE_ERROR, Value, err};
use crate::model::CellError::{NA, Num};
use crate::model::CellValue::{Blank, Error};
use crate::model::{CellAddress, CellError, CellValue, ExcelText};
use poirot_functions::text::units;

/// Convert a selected member to UTF-16 without a lossy UTF-8 projection.
pub(in crate::eval::adapters) fn units(
    host: &mut CallHost<'_, '_>,
    operand: &ScalarOperand,
) -> FunctionResult<Vec<u16>> {
    ArgumentCoercion::new(host).coerce_text_units(&operand.value)
}

/// Coerce a modern Excel integer, retaining checked bounds before truncation.
pub(in crate::eval::adapters) fn integer(number: f64) -> FunctionResult<i32> {
    poirot_functions::coercion::compatibility::coerce_text_integer_number(number)?;
    Ok(number.floor() as i32)
}

/// Translate an independent text planner's failure into evaluator channels.
pub(in crate::eval::adapters) fn plan<T>(
    host: &CallHost<'_, '_>,
    result: Result<T, poirot_functions::text::TextPlanError>,
) -> FunctionResult<T> {
    result.map_err(|error| match error {
        poirot_functions::text::TextPlanError::Value => VALUE_ERROR.into(),
        _ => host.size_overflow().into(),
    })
}

/// Admit the buffers and work requested by a pure UTF-16 planner.
pub(in crate::eval::adapters) fn reserve(
    host: &mut CallHost<'_, '_>,
    reservation: units::Reservation,
) -> FunctionResult<()> {
    host.check_text_capacity(reservation.text_bytes)?;
    host.check_values_capacity(reservation.elements)?;
    host.charge_work(reservation.work as u64)?;
    Ok(())
}

/// Assemble lossless text without implicitly allocating a UTF-8 copy.
pub(in crate::eval::adapters) fn value(output: Vec<u16>) -> CellValue {
    CellValue::TextUtf16(ExcelText::from_units(output))
}

/// Copy one admitted UTF-16 selection into a result cell.
pub(in crate::eval::adapters) fn copied(
    host: &mut CallHost<'_, '_>,
    text: &[u16],
) -> FunctionResult<CellValue> {
    let reservation = plan(host, units::Reservation::units(text.len(), text.len()))?;
    reserve(host, reservation)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(text.len())
        .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
    // SAFETY: the entire selected slice was reserved immediately above.
    output.extend_from_slice(text);
    Ok(value(output))
}

/// A row-major rectangle produced by one text or regex calculation.
/// Shared publication checks its dimensions and converts it into a scalar or array result.
pub(in crate::eval::adapters) struct Expansion {
    pub(in crate::eval::adapters) values: Vec<CellValue>,
    pub(in crate::eval::adapters) rows: usize,
    pub(in crate::eval::adapters) columns: usize,
}

/// A typed member algorithm whose result may expand along either output axis.
pub(in crate::eval::adapters) trait ExpansionKernel {
    /// Source roles admitted before this algorithm reads members.
    type Signature: super::tuple_signature::Signature;

    /// Produce a checked flat result in row-major order for one selected member.
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<Expansion>;
}

/// Assemble a row-major rectangle with checked geometry and fallible storage.
pub(in crate::eval::adapters) fn rectangle(
    host: &mut CallHost<'_, '_>,
    output: Expansion,
    address: CellAddress,
) -> FunctionResult<Value> {
    let Expansion {
        values,
        rows,
        columns,
    } = output;
    if rows == 1 && columns == 1 {
        return values
            .into_iter()
            .next()
            .map(Value::Scalar)
            .ok_or(VALUE_ERROR.into());
    }
    if rows == 0 || columns == 0 {
        return Err(VALUE_ERROR.into());
    }
    if rows > crate::model::MAX_ROWS as usize || columns > crate::model::MAX_COLUMNS as usize {
        return Err(Num.into());
    }
    let count = rows
        .checked_mul(columns)
        .ok_or_else(|| host.size_overflow())?;
    if values.len() != count {
        return Err(VALUE_ERROR.into());
    }
    host.check_values(count)?;
    host.check_values(rows)?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(rows)
        .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
    let mut values = values.into_iter();
    for _ in 0..rows {
        let mut row = Vec::new();
        row.try_reserve_exact(columns)
            .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
        for _ in 0..columns {
            // SAFETY: exactly columns slots were reserved for this row above.
            row.push(values.next().ok_or(VALUE_ERROR)?);
        }
        // SAFETY: exactly rows slots were reserved before this loop.
        result.push(row);
    }
    Ok(Value::Array(ArrayView::anchored(result, Some(address))))
}

/// Expand a scalar source or broadcast single-cell results, rejecting nested arrays.
pub(in crate::eval::adapters) fn expand<K: ExpansionKernel>(
    host: &mut CallHost<'_, '_>,
    address: CellAddress,
    kernel: K,
) -> FunctionResult<Value> {
    use super::tuple_signature::Signature;
    let (lift, arguments) = K::Signature::admit(host)?;
    if lift.dimensions() == (1, 1) {
        let output = kernel.apply(
            &mut MemberContext {
                host,
                lift: &lift,
                row: 0,
                column: 0,
            },
            &arguments,
        )?;
        return rectangle(host, output, address);
    }
    lift.evaluate(host, address, |host, row, column| {
        let output = kernel.apply(
            &mut MemberContext {
                host,
                lift: &lift,
                row,
                column,
            },
            &arguments,
        )?;
        if output.rows != 1 || output.columns != 1 || output.values.len() != 1 {
            return Err(CellError::Calc.into());
        }
        output.values.into_iter().next().ok_or(VALUE_ERROR.into())
    })
}

/// A scalar text result or a demand for a deferred fallback source.
pub(in crate::eval::adapters) enum Selection {
    Value(CellValue),
    Fallback,
}

/// Typed extraction with a fallback that may enlarge the output rectangle.
pub(in crate::eval::adapters) trait FallbackKernel {
    /// Source roles admitted before this algorithm reads members.
    type Signature: super::tuple_signature::Signature;

    /// Return the deferred fallback role without preparing its source.
    fn fallback(arguments: &Self::Signature) -> Option<&super::with_demand::Deferred<Broadcast>>;

    /// Preserve an original result channel when no member needs the fallback.
    fn retained(
        &self,
        _host: &mut CallHost<'_, '_>,
        _arguments: &Self::Signature,
    ) -> FunctionResult<Option<Value>> {
        Ok(None)
    }

    /// Compute one extraction while leaving fallback evaluation deferred.
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<Selection>;
}

/// Complete the extraction mask before evaluating and broadcasting a needed fallback.
pub(in crate::eval::adapters) fn fallback<K: FallbackKernel>(
    host: &mut CallHost<'_, '_>,
    address: CellAddress,
    kernel: K,
) -> FunctionResult<Value> {
    use super::tuple_signature::Signature;
    use crate::eval::vm::Shape;
    let (lift, arguments) = K::Signature::admit(host)?;
    let (rows, columns) = lift.dimensions();
    let count = rows
        .checked_mul(columns)
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(count)?;
    let mut mask = Vec::new();
    mask.try_reserve_exact(count)
        .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
    let mut needed = false;
    let result = lift.evaluate(host, address, |host, row, column| {
        let selected = kernel.apply(
            &mut MemberContext {
                host,
                lift: &lift,
                row,
                column,
            },
            &arguments,
        );
        let (missing, value) = match selected {
            Ok(Selection::Value(value)) => (false, value),
            Ok(Selection::Fallback) => {
                needed = true;
                (true, Blank)
            }
            Err(FunctionError::Spreadsheet(error)) => (false, Error(error)),
            Err(error) => return Err(error),
        };
        mask.try_reserve(1)
            .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
        // SAFETY: one additional mask slot was reserved immediately above.
        mask.push(missing);
        Ok(value)
    })?;
    if !needed {
        return Ok(kernel.retained(host, &arguments)?.unwrap_or(result));
    }
    let prepared = K::fallback(&arguments)
        .map(|role| role.prepare(host))
        .transpose()?;
    resolve_fallback(host, Shape { rows, columns }, result, &mask, prepared)
}

/// Resolve a completed mask while reading only demanded fallback reference members.
/// A scalar source selecting its fallback preserves the original result channel.
pub(in crate::eval::adapters) fn resolve_fallback(
    host: &mut CallHost<'_, '_>,
    source_shape: crate::eval::vm::Shape,
    result: Value,
    mask: &[bool],
    prepared: Option<super::prepared::Prepared<Broadcast>>,
) -> FunctionResult<Value> {
    use crate::eval::vm::Shape;
    if source_shape.rows == 1 && source_shape.columns == 1 && mask.first() == Some(&true) {
        return Ok(prepared
            .map(super::prepared::Prepared::into_value)
            .unwrap_or_else(|| err(NA)));
    }
    let branch = prepared
        .map(|value| {
            let shape = value.shape().ok_or(VALUE_ERROR)?;
            Ok::<_, FunctionError>((value, shape))
        })
        .transpose()?;
    let Shape { rows, columns } = source_shape;
    let shape = branch.as_ref().map_or(source_shape, |(_, branch)| Shape {
        rows: rows.max(branch.rows),
        columns: columns.max(branch.columns),
    });
    SelectionAccess::new(host)
        .array(shape, |reader, row, column| {
            let source_row = if rows == 1 { 0 } else { row };
            let source_column = if columns == 1 { 0 } else { column };
            let index = source_row
                .checked_mul(columns)
                .and_then(|n| n.checked_add(source_column));
            if source_row >= rows || source_column >= columns {
                return Ok(Error(NA));
            }
            if index.and_then(|i| mask.get(i)).copied().unwrap_or(false) {
                match &branch {
                    Some((value, shape)) => value.read(reader, *shape, row, column),
                    None => Ok(Error(NA)),
                }
            } else {
                reader.member(&result, source_shape, row, column)
            }
        })
        .map_err(Into::into)
}

/// Render a checked positional scalar argument through shared text coercion.
pub(in crate::eval::adapters) fn argument(
    host: &mut CallHost<'_, '_>,
    arguments: &[CellValue],
    index: usize,
) -> FunctionResult<String> {
    ArgumentCoercion::new(host).coerce_scalar_text(arguments.get(index).ok_or(VALUE_ERROR)?)
}

/// Coerce a bounded Excel integer argument while retaining its fractional value.
pub(in crate::eval::adapters) fn integer_argument(
    host: &mut CallHost<'_, '_>,
    arguments: &[CellValue],
    index: usize,
) -> FunctionResult<f64> {
    let value = arguments.get(index).ok_or(VALUE_ERROR)?;
    let number = SelectionAccess::new(host).number(value)?;
    poirot_functions::coercion::compatibility::coerce_text_integer_number(number)
        .map_err(Into::into)
}

/// Materialize text collections in source order without reducing a delimiter set.
pub(in crate::eval::adapters) fn collection_units(
    host: &mut CallHost<'_, '_>,
    roles: impl IntoIterator<Item = super::whole_collection::WholeCollection>,
) -> FunctionResult<Vec<Vec<u16>>> {
    super::collections::fold(host, roles, Vec::new(), |host, values, operand| {
        let text = units(host, &operand)?;
        values
            .try_reserve(1)
            .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
        // SAFETY: one additional text slot was reserved immediately above.
        values.push(text);
        Ok(())
    })
}
