use super::argument_role::ArgumentRole;
use super::controls::{self, IntegerControl};
use super::matrices;
use crate::eval::host::{CallHost, OperandDescription, OperandReader, ScalarOperand};
use crate::eval::values::{FunctionResult, VALUE_ERROR, core_projection};
use poirot_functions::matrix::Matrix;

/// Argument role for an entire source collection, such as a statistical sample.
/// It provides ordered member access while leaving output dimensions to other roles.
#[derive(Clone, Copy)]
pub(in crate::eval::adapters) struct WholeCollection {
    pub(super) argument: usize,
}

impl ArgumentRole for WholeCollection {
    const BROADCAST: bool = false;

    fn argument(&self) -> usize {
        self.argument
    }

    fn bind(argument: usize, _member: usize, _present: bool) -> Self {
        Self { argument }
    }
}

impl WholeCollection {
    /// Describe the bound collection without resolving a member.
    pub(in crate::eval::adapters) fn describe(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<OperandDescription> {
        OperandReader::new(host).describe(self.argument)
    }

    /// Read an aligned cell through the bound collection's source identity.
    pub(in crate::eval::adapters) fn read_at(
        &self,
        host: &mut CallHost<'_, '_>,
        row: usize,
        column: usize,
    ) -> FunctionResult<ScalarOperand> {
        OperandReader::new(host).read_cell(self.argument, row, column)
    }

    /// Return the bounded collection length using the shared host admission.
    pub(in crate::eval::adapters) fn len(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<usize> {
        OperandReader::new(host).collection_len(self.argument)
    }

    /// Read an ordered member while retaining its original source classification.
    pub(in crate::eval::adapters) fn read(
        &self,
        host: &mut CallHost<'_, '_>,
        offset: usize,
    ) -> FunctionResult<ScalarOperand> {
        OperandReader::new(host).collection_cell(self.argument, offset)
    }

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
            core_projection::coerce_operand_boolean(&value).map_err(Into::into)
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
            let integer = controls::integer(host, &value, mode, VALUE_ERROR)?;
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
            let shape = OperandReader::new(host).describe(self.argument())?;
            if shape.sheets != 1 || shape.columns != 1 || shape.rows != rows {
                return Err(VALUE_ERROR.into());
            }
        }
        host.check_values(rows)?;
        let mut selected = Vec::new();
        selected
            .try_reserve_exact(rows)
            .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
        for row in start..rows {
            if !present
                || core_projection::coerce_operand_boolean(
                    &OperandReader::new(host)
                        .read_cell(self.argument(), row, 0)?
                        .value,
                )?
            {
                // SAFETY: rows slots were reserved; the bounded loop appends at most rows positions.
                selected.push(row);
            }
        }
        Ok(selected)
    }
}
