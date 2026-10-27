use super::argument_role::ArgumentRole;
use super::controls::{self, EmptyControl, IntegerControl};
use super::signatures::{NumericParameter, NumericSignature};
use crate::eval::host::{CallHost, ScalarOperand};
use crate::eval::values::FunctionResult;
use crate::model::CellValue::Blank;
use crate::model::{CellError, CellValue};

/// Argument role for one scalar control selected with the host's scalar-intersection rules.
/// Its source shape does not enlarge the output rectangle.
pub(in crate::eval::adapters) struct FixedScalar {
    pub(super) argument: usize,
}

impl ArgumentRole for FixedScalar {
    const BROADCAST: bool = false;

    fn argument(&self) -> usize {
        self.argument
    }

    fn bind(argument: usize, _member: usize, _present: bool) -> Self {
        Self { argument }
    }
}

impl FixedScalar {
    /// Bind a fixed slot outside a rectangular signature.
    pub(in crate::eval::adapters) fn at(argument: usize) -> Self {
        Self { argument }
    }

    /// Resolve using the host's existing scalar intersection and omission rules.
    pub(in crate::eval::adapters) fn read(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> FunctionResult<ScalarOperand> {
        use poirot_functions::scalar::Input;
        host.input(self.argument, crate::eval::host::MissingPolicy::Blank)
            .force()
    }

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
        controls::integer(host, &operand.value, IntegerControl::Truncate, error).map(Some)
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
            poirot_functions::coercion::compatibility::coerce_array_member(operand.value)
        })
    }
}
