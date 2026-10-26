//! Shared lossless text coercion, integer selection, and output admission.
use super::{Broadcast, MemberContext, NumericParameter, NumericSignature, Variadic};
use crate::host::{CallHost, ScalarOperand};
use crate::*;
use poirot_functions_text::units;
use poirot_model::ExcelText;

/// Convert a selected member to UTF-16 without a lossy UTF-8 projection.
pub(in crate::adapters) fn units(
    host: &mut CallHost<'_, '_>,
    operand: &ScalarOperand,
) -> FunctionResult<Vec<u16>> {
    host.coercev2_text_units(&operand.value)
}
/// Coerce a modern Excel integer, retaining checked bounds before truncation.
pub(in crate::adapters) fn integer(number: f64) -> FunctionResult<i32> {
    poirot_coercionv2::compatibility::coercev2_text_integer_number(
        number,
        poirot_coercionv2::compatibility::TextIntegerPolicy::Checked,
    )?;
    Ok(number.floor() as i32)
}
/// Translate an independent text planner's failure into evaluator channels.
pub(in crate::adapters) fn plan<T>(
    host: &CallHost<'_, '_>,
    result: Result<T, units::PlanFailure>,
) -> FunctionResult<T> {
    result.map_err(|error| match error {
        units::PlanFailure::Value => VALUE_ERROR.into(),
        _ => host.size_overflow().into(),
    })
}
/// Admit the buffers and work requested by a pure UTF-16 planner.
pub(in crate::adapters) fn reserve(
    host: &mut CallHost<'_, '_>,
    reservation: units::Reservation,
) -> FunctionResult<()> {
    host.check_text_capacity(reservation.text_bytes)?;
    host.check_values_capacity(reservation.elements)?;
    host.charge_work(reservation.work as u64)?;
    Ok(())
}
/// Assemble lossless text without implicitly allocating a UTF-8 copy.
pub(in crate::adapters) fn value(output: Vec<u16>) -> CellValue {
    CellValue::TextUtf16(ExcelText::from_units(output))
}
/// Copy one admitted UTF-16 selection into a result cell.
pub(in crate::adapters) fn copied(
    host: &mut CallHost<'_, '_>,
    text: &[u16],
) -> FunctionResult<CellValue> {
    let reservation = plan(host, units::Reservation::units(text.len(), text.len()))?;
    reserve(host, reservation)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(text.len())
        .map_err(|_| host.allocation_failure())?;
    // SAFETY: the entire selected slice was reserved immediately above.
    output.extend_from_slice(text);
    Ok(value(output))
}
impl Variadic<Broadcast> {
    /// Materialize source-ordered scalar members with shared error propagation.
    pub(in crate::adapters) fn scalar_values(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
    ) -> FunctionResult<Vec<CellValue>> {
        context.host.check_values(self.len())?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(self.len())
            .map_err(|_| context.host.allocation_failure())?;
        for role in self.iter() {
            let operand = role.read(context)?;
            if let Error(error) = operand.value {
                return Err(error.into());
            }
            values
                .try_reserve(1)
                .map_err(|_| context.host.allocation_failure())?;
            // SAFETY: one additional slot was reserved immediately above.
            values.push(operand.value);
        }
        Ok(values)
    }
    /// Coerce a numeric option without truncating selector values before validation.
    pub(in crate::adapters) fn number(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        index: usize,
        default: Option<f64>,
    ) -> FunctionResult<f64> {
        match self.get(index) {
            Some(role) => role.number(context, default),
            None => default.ok_or(VALUE_ERROR.into()),
        }
    }
    /// Read one present typed role in the current output coordinate.
    pub(in crate::adapters) fn operand(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        index: usize,
    ) -> FunctionResult<ScalarOperand> {
        self.get(index).ok_or(VALUE_ERROR)?.read(context)
    }
    /// Render a typed role without losing unpaired UTF-16 units.
    pub(in crate::adapters) fn text_units(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        index: usize,
    ) -> FunctionResult<Vec<u16>> {
        let operand = self.operand(context, index)?;
        units(context.host, &operand)
    }
    /// Coerce an omitted integer slot as zero; only an absent slot uses the default.
    pub(in crate::adapters) fn text_integer(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        index: usize,
        default: Option<i32>,
    ) -> FunctionResult<i32> {
        let parameter = default
            .map_or_else(NumericParameter::required, |value| {
                NumericParameter::constant(f64::from(value))
            })
            .missing_as_blank();
        let numbers =
            NumericSignature::new(&[parameter]).numbers_roles(context, [self.get(index)])?;
        integer(numbers.get(0)?)
    }
}

/// A variable-size result rectangle returned by an independent scalar algorithm.
pub(in crate::adapters) struct Expansion {
    pub(in crate::adapters) values: Vec<CellValue>,
    pub(in crate::adapters) rows: usize,
    pub(in crate::adapters) columns: usize,
}
/// A typed member algorithm whose result may expand along either output axis.
pub(in crate::adapters) trait ExpansionKernel {
    /// Source roles admitted before this algorithm reads members.
    type Signature: super::Signature;
    /// Produce a checked flat result in row-major order for one selected member.
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<Expansion>;
}
/// Assemble a row-major rectangle with checked geometry and fallible storage.
pub(in crate::adapters) fn rectangle(
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
    if rows > poirot_model::MAX_ROWS as usize || columns > poirot_model::MAX_COLUMNS as usize {
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
        .map_err(|_| host.allocation_failure())?;
    let mut values = values.into_iter();
    for _ in 0..rows {
        let mut row = Vec::new();
        row.try_reserve_exact(columns)
            .map_err(|_| host.allocation_failure())?;
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
pub(in crate::adapters) fn expand<K: ExpansionKernel>(
    host: &mut CallHost<'_, '_>,
    address: CellAddress,
    kernel: K,
) -> FunctionResult<Value> {
    use super::Signature;
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
pub(in crate::adapters) enum Selection {
    Value(CellValue),
    Fallback,
}
/// Typed extraction with a fallback that may enlarge the output rectangle.
pub(in crate::adapters) trait FallbackKernel {
    /// Source roles admitted before this algorithm reads members.
    type Signature: super::Signature;
    /// Return the deferred fallback role without preparing its source.
    fn fallback(arguments: &Self::Signature) -> Option<&super::Deferred<Broadcast>>;
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
pub(in crate::adapters) fn fallback<K: FallbackKernel>(
    host: &mut CallHost<'_, '_>,
    address: CellAddress,
    kernel: K,
) -> FunctionResult<Value> {
    use super::Signature;
    use crate::array_operator::Shape;
    let (lift, arguments) = K::Signature::admit(host)?;
    let (rows, columns) = lift.dimensions();
    let count = rows
        .checked_mul(columns)
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(count)?;
    let mut mask = Vec::new();
    mask.try_reserve_exact(count)
        .map_err(|_| host.allocation_failure())?;
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
            Err(FunctionFailure::Spreadsheet(error)) => (false, Error(error)),
            Err(error) => return Err(error),
        };
        mask.try_reserve(1).map_err(|_| host.allocation_failure())?;
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
pub(in crate::adapters) fn resolve_fallback(
    host: &mut CallHost<'_, '_>,
    source_shape: crate::array_operator::Shape,
    result: Value,
    mask: &[bool],
    prepared: Option<super::Prepared<Broadcast>>,
) -> FunctionResult<Value> {
    use crate::array_operator::Shape;
    if source_shape.rows == 1 && source_shape.columns == 1 && mask.first() == Some(&true) {
        return Ok(prepared
            .map(super::Prepared::into_value)
            .unwrap_or_else(|| err(NA)));
    }
    let branch = prepared
        .map(|value| {
            let shape = value.shape(host).ok_or(VALUE_ERROR)?;
            Ok::<_, FunctionFailure>((value, shape))
        })
        .transpose()?;
    let Shape { rows, columns } = source_shape;
    let shape = branch.as_ref().map_or(source_shape, |(_, branch)| Shape {
        rows: rows.max(branch.rows),
        columns: columns.max(branch.columns),
    });
    host.selected_array(shape, |reader, row, column| {
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
pub(in crate::adapters) fn argument(
    host: &mut CallHost<'_, '_>,
    arguments: &[CellValue],
    index: usize,
) -> FunctionResult<String> {
    host.coercev2_scalar_text(arguments.get(index).ok_or(VALUE_ERROR)?)
}
/// Coerce a bounded Excel integer argument while retaining its fractional value.
pub(in crate::adapters) fn integer_argument(
    host: &mut CallHost<'_, '_>,
    arguments: &[CellValue],
    index: usize,
) -> FunctionResult<f64> {
    let value = arguments.get(index).ok_or(VALUE_ERROR)?;
    let number = host.selected_number(value)?;
    poirot_coercionv2::compatibility::coercev2_text_integer_number(
        number,
        poirot_coercionv2::compatibility::TextIntegerPolicy::Checked,
    )
    .map_err(Into::into)
}

/// Materialize text collections in source order without reducing a delimiter set.
pub(in crate::adapters) fn collection_units(
    host: &mut CallHost<'_, '_>,
    roles: impl IntoIterator<Item = super::WholeCollection>,
) -> FunctionResult<Vec<Vec<u16>>> {
    super::collections::fold(host, roles, Vec::new(), |host, values, operand| {
        let text = units(host, &operand)?;
        values
            .try_reserve(1)
            .map_err(|_| host.allocation_failure())?;
        // SAFETY: one additional text slot was reserved immediately above.
        values.push(text);
        Ok(())
    })
}
