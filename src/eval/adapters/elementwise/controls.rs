//! Typed fixed controls and selector collections, independent of a family's array geometry.
use super::argument_role::ArgumentRole;
use super::fixed_scalar::FixedScalar;
use super::whole_collection::WholeCollection;
use crate::eval::host::{ArgumentCoercion, CallHost, NumericPolicy, SelectionAccess};
use crate::eval::values::{FunctionResult, VALUE_ERROR, Value};
use crate::model::CellError::Ref;
use crate::model::CellValue::Error;
use crate::model::{CellError, CellValue};

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
pub(super) fn integer(
    host: &mut CallHost<'_, '_>,
    value: &CellValue,
    mode: IntegerControl,
    error: CellError,
) -> FunctionResult<i64> {
    let number =
        ArgumentCoercion::new(host).coerce_scalar_number(value, NumericPolicy::PinnedPoi)?;
    if !number.is_finite()
        || number.abs() >= i64::MAX as f64
        || mode == IntegerControl::Exact && number.fract() != 0.0
    {
        return Err(error.into());
    }
    Ok(number.trunc() as i64)
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
    ) -> FunctionResult<Option<super::prepared::Prepared<FixedScalar>>> {
        if index >= self.supplied {
            return Ok(None);
        }
        let argument = index.checked_add(1).ok_or(VALUE_ERROR)?;
        if SelectionAccess::new(host).is_missing(argument) {
            return Ok(Some(super::prepared::Prepared::new(Value::Missing)));
        }
        Ok(Some(
            super::with_demand::Deferred::<FixedScalar>::bind(argument, 0, true).prepare(host)?,
        ))
    }

    /// Retain all supplied fixed controls in source order before any coercion.
    fn prepare_all(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<[Option<super::prepared::Prepared<FixedScalar>>; N]> {
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
        source: &super::prepared::Prepared<super::source::SourceIdentity>,
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
                if !SelectionAccess::new(host).is_missing(3)
                    && !matches!(value.value, Value::Missing) =>
            {
                let scalar = host.select_legacy_scalar(value.into_value())?;
                Some(SelectionAccess::new(host).number(&scalar)?)
            }
            _ => None,
        };
        Ok(Ordinals {
            row,
            column,
            area: poirot_functions::coercion::lookup::coerce_index_area(
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
    value: super::prepared::Prepared<FixedScalar>,
) -> FunctionResult<usize> {
    let scalar = host.select_legacy_scalar(value.into_value())?;
    Ok(
        poirot_functions::coercion::compatibility::coerce_index_ordinal(
            SelectionAccess::new(host).number(&scalar)?,
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
        source: &super::prepared::Prepared<super::source::SourceIdentity>,
    ) -> FunctionResult<OffsetValues> {
        let geometry = super::source::SourceIdentity::geometry(source.value())?;
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
    value: Option<super::prepared::Prepared<FixedScalar>>,
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
