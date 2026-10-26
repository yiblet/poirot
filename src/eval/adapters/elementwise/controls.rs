//! Typed fixed controls and selector collections, independent of a family's array geometry.
use super::{
    ArgumentRole, FixedScalar, NumericParameter, NumericSignature, WholeCollection, matrices,
};
use crate::eval::host::{CallHost, NumericPolicy};
use crate::eval::*;
use poirot_functions::matrix::Matrix;

/// Source values that use an optional integer control's absent default.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::eval::adapters) enum EmptyControl {
    /// Only absent and explicitly omitted arguments have no integer value.
    Omission,
    /// Blank cells also have no integer value.
    BlankOrOmission,
}
/// Whether integer selectors accept a fractional numeric source.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::eval::adapters) enum IntegerControl {
    /// Truncate finite values after checking the signed integer envelope.
    Truncate,
    /// Require an exactly integral source value.
    Exact,
}
/// Coerce one integer selector through the host's numeric policy and checked integer envelope.
fn integer(
    host: &mut CallHost<'_, '_>,
    value: &CellValue,
    mode: IntegerControl,
    error: CellError,
) -> FunctionResult<i64> {
    let number = host.coercev2_scalar_number(value, NumericPolicy::PinnedPoi)?;
    if !number.is_finite()
        || number.abs() >= i64::MAX as f64
        || mode == IntegerControl::Exact && number.fract() != 0.0
    {
        return Err(error.into());
    }
    Ok(number.trunc() as i64)
}
impl FixedScalar {
    /// Prepare one numeric control using the same omission and coercion rules as broadcast roles.
    pub(in crate::eval::adapters) fn number(
        &self,
        host: &mut CallHost<'_, '_>,
        parameter: NumericParameter,
    ) -> FunctionResult<f64> {
        NumericSignature::new(&[parameter])
            .scalars(host, self.argument())?
            .get(0)
    }
    /// Prepare one logical control, defaulting absent or explicitly omitted sources only.
    pub(in crate::eval::adapters) fn boolean(
        &self,
        host: &mut CallHost<'_, '_>,
        default: bool,
    ) -> FunctionResult<bool> {
        Ok(self.number(host, NumericParameter::boolean_default(default))? != 0.0)
    }
    /// Prepare a signed optional integer while retaining omission and blank distinctions.
    pub(in crate::eval::adapters) fn optional_integer(
        &self,
        host: &mut CallHost<'_, '_>,
        empty: EmptyControl,
        error: CellError,
    ) -> FunctionResult<Option<i64>> {
        if self.argument() >= host.len() {
            return Ok(None);
        }
        let operand = self.read(host)?;
        if operand.missing || empty == EmptyControl::BlankOrOmission && operand.value == Blank {
            return Ok(None);
        }
        integer(host, &operand.value, IntegerControl::Truncate, error).map(Some)
    }
    /// Prepare a mixed scalar value without projecting UTF-16 text or evaluating an absent source.
    pub(in crate::eval::adapters) fn value(
        &self,
        host: &mut CallHost<'_, '_>,
        default: CellValue,
    ) -> FunctionResult<CellValue> {
        if self.argument() >= host.len() {
            return Ok(default);
        }
        let operand = self.read(host)?;
        Ok(if operand.missing {
            default
        } else {
            poirot_functions::coercion::compatibility::coercev2_array_member(operand.value)
        })
    }
}
impl WholeCollection {
    /// Bind a complete selector source without reading or coercing its members.
    pub(in crate::eval::adapters) fn at(argument: usize) -> Self {
        Self::bind(argument, 0, true)
    }
    /// Prepare a rectangular logical mask after materializing its original values in source order.
    pub(in crate::eval::adapters) fn booleans<S>(
        &self,
        host: &mut CallHost<'_, '_>,
        validate: impl FnOnce(usize, usize) -> FunctionResult<S>,
    ) -> FunctionResult<(S, Matrix<bool>)> {
        let source = matrices::matrix(host, self.argument(), matrices::Members::Original)?;
        let shape = validate(source.rows(), source.cols())?;
        let values = matrices::convert(host, source, |_, value| {
            core_projection::coercev2_operand_boolean(&value).map_err(Into::into)
        })?;
        Ok((shape, values))
    }
    /// Prepare a rectangular integer selector while retaining its geometry for domain validation.
    pub(in crate::eval::adapters) fn integers(
        &self,
        host: &mut CallHost<'_, '_>,
        mode: IntegerControl,
        mut validate: impl FnMut(i64, usize) -> FunctionResult<()>,
    ) -> FunctionResult<Matrix<i64>> {
        let source = matrices::matrix(host, self.argument(), matrices::Members::Original)?;
        let count = source.values().len();
        matrices::convert(host, source, |host, value| {
            let integer = integer(host, &value, mode, VALUE_ERROR)?;
            validate(integer, count)?;
            Ok(integer)
        })
    }
    /// Prepare included row ordinals without reading a consumed header row's mask member.
    pub(in crate::eval::adapters) fn selected_rows(
        &self,
        host: &mut CallHost<'_, '_>,
        rows: usize,
        start: usize,
    ) -> FunctionResult<Vec<usize>> {
        let present = self.argument() < host.len()
            && host.kind(self.argument())? != crate::eval::host::OperandKind::Missing;
        if present {
            let shape = host.describe(self.argument())?;
            if shape.sheets != 1 || shape.columns != 1 || shape.rows != rows {
                return Err(VALUE_ERROR.into());
            }
        }
        host.check_values(rows)?;
        let mut selected = Vec::new();
        selected
            .try_reserve_exact(rows)
            .map_err(|_| host.allocation_failure())?;
        for row in start..rows {
            if !present
                || core_projection::coercev2_operand_boolean(
                    &host.read_cell(self.argument(), row, 0)?.value,
                )?
            {
                // SAFETY: rows slots were reserved; the bounded loop appends at most rows positions.
                selected.push(row);
            }
        }
        Ok(selected)
    }
}
/// Prepare consecutive selector collections, preserving argument and member order.
pub(in crate::eval::adapters) fn integer_selectors(
    host: &mut CallHost<'_, '_>,
    roles: impl IntoIterator<Item = WholeCollection>,
) -> FunctionResult<Vec<i64>> {
    let mut selectors = Vec::new();
    for role in roles {
        let values = role
            .integers(host, IntegerControl::Truncate, |_, _| Ok(()))?
            .into_values();
        let count = selectors
            .len()
            .checked_add(values.len())
            .ok_or_else(|| host.size_overflow())?;
        host.check_values(count)?;
        selectors
            .try_reserve(values.len())
            .map_err(|_| host.allocation_failure())?;
        // SAFETY: values.len additional slots were reserved immediately above.
        selectors.extend(values);
    }
    Ok(selectors)
}

/// Binds the fixed control slots following a reference source for INDEX or OFFSET.
/// It records how many controls were supplied so preparation can distinguish absence from explicit omission.
pub(in crate::eval::adapters) struct FixedControls<const N: usize> {
    supplied: usize,
}
/// Converted INDEX selectors passed to reference projection.
/// Rows and columns remain one-based, with zero selecting an entire axis; the selected union area is zero-based.
pub(in crate::eval::adapters) struct Ordinals {
    pub(in crate::eval::adapters) row: usize,
    pub(in crate::eval::adapters) column: usize,
    pub(in crate::eval::adapters) area: usize,
}
/// Ordered row, column, and area selectors for INDEX.
pub(in crate::eval::adapters) type OrdinalControls = FixedControls<3>;
/// Ordered row/column displacements and optional output dimensions for OFFSET.
pub(in crate::eval::adapters) type OffsetControls = FixedControls<4>;
impl<const N: usize> FixedControls<N> {
    /// Bind the present fixed controls without evaluating any expression.
    pub(in crate::eval::adapters) fn new(arguments: usize) -> Self {
        Self {
            supplied: arguments.saturating_sub(1).min(N),
        }
    }
    /// Retain one supplied fixed control without coercion or a repeated expression read.
    fn prepare_one(
        &self,
        host: &mut CallHost<'_, '_>,
        index: usize,
    ) -> FunctionResult<Option<super::Prepared<FixedScalar>>> {
        if index >= self.supplied {
            return Ok(None);
        }
        let argument = index.checked_add(1).ok_or(VALUE_ERROR)?;
        if host.selected_is_missing(argument) {
            return Ok(Some(super::Prepared::new(Value::Missing)));
        }
        Ok(Some(
            super::Deferred::<FixedScalar>::bind(argument, 0, true).prepare(host)?,
        ))
    }
    /// Retain all supplied fixed controls in source order before any coercion.
    fn prepare_all(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<[Option<super::Prepared<FixedScalar>>; N]> {
        let mut forced = std::array::from_fn(|_| None);
        for (index, slot) in forced.iter_mut().enumerate().take(self.supplied) {
            *slot = self.prepare_one(host, index)?;
        }
        Ok(forced)
    }
}
impl super::source::Controls for OrdinalControls {
    /// Checked row, column, and union-area ordinals.
    type Values = Ordinals;
    fn prepare(
        &self,
        host: &mut CallHost<'_, '_>,
        source: &super::Prepared<super::SourceIdentity>,
    ) -> FunctionResult<Ordinals> {
        let [row, column, area] = self.prepare_all(host)?;
        if let Value::Scalar(Error(error)) | Value::SelectedScalar(Error(error), _) = source.value()
        {
            return Err((*error).into());
        }
        let column = column.map(|v| ordinal(host, v)).transpose()?.unwrap_or(1);
        let row = ordinal(host, row.ok_or(VALUE_ERROR)?)?;
        let area = match area {
            Some(value)
                if !host.selected_is_missing(3) && !matches!(value.value, Value::Missing) =>
            {
                let scalar = host.select_legacy_scalar(value.into_value())?;
                Some(host.selected_number(&scalar)?)
            }
            _ => None,
        };
        Ok(Ordinals {
            row,
            column,
            area: poirot_functions::coercion::lookup::coercev2_index_area(
                area,
                source.area_count(),
                poirot_functions::coercion::lookup::IndexAreaDefault::FirstArea,
            )?,
        })
    }
}
/// Select and coerce one retained ordinal after all source expressions were forced.
fn ordinal(
    host: &mut CallHost<'_, '_>,
    value: super::Prepared<FixedScalar>,
) -> FunctionResult<usize> {
    let scalar = host.select_legacy_scalar(value.into_value())?;
    Ok(
        poirot_functions::coercion::compatibility::coercev2_index_ordinal(
            host.selected_number(&scalar)?,
        )?,
    )
}

/// Converted OFFSET displacement and result dimensions passed to reference projection.
/// Signed values retain the source's logical coordinates until the projection checks worksheet bounds.
pub(in crate::eval::adapters) struct OffsetValues {
    pub(in crate::eval::adapters) row: i64,
    pub(in crate::eval::adapters) column: i64,
    pub(in crate::eval::adapters) height: i64,
    pub(in crate::eval::adapters) width: i64,
}
impl super::source::Controls for OffsetControls {
    /// Checked signed displacement and optional-dimension defaults.
    type Values = OffsetValues;
    fn prepare(
        &self,
        host: &mut CallHost<'_, '_>,
        source: &super::Prepared<super::SourceIdentity>,
    ) -> FunctionResult<OffsetValues> {
        let geometry = super::SourceIdentity::geometry(source.value())?;
        // Retain staged source/row/column/width/height admission until native Excel
        // evidence justifies changing its observable error and demand precedence.
        let row = {
            let value = self.prepare_one(host, 0)?;
            retained_integer(host, value)?
        }
        .unwrap_or(0);
        let column = {
            let value = self.prepare_one(host, 1)?;
            retained_integer(host, value)?
        }
        .unwrap_or(0);
        let width = {
            let value = self.prepare_one(host, 3)?;
            retained_integer(host, value)?
        }
        .unwrap_or(i64::try_from(geometry.columns).map_err(|_| Ref)?);
        let height = {
            let value = self.prepare_one(host, 2)?;
            retained_integer(host, value)?
        }
        .unwrap_or(i64::try_from(geometry.rows).map_err(|_| Ref)?);
        Ok(OffsetValues {
            row,
            column,
            height,
            width,
        })
    }
}
/// Coerce a retained optional integer without repeating expression evaluation.
fn retained_integer(
    host: &mut CallHost<'_, '_>,
    value: Option<super::Prepared<FixedScalar>>,
) -> FunctionResult<Option<i64>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if matches!(value.value, Value::Missing) {
        return Ok(None);
    }
    let scalar = host.select_legacy_scalar(value.into_value())?;
    // Fractional truncation is corroborated by Calc; native Excel is unverified.
    integer(host, &scalar, IntegerControl::Truncate, Ref).map(Some)
}
