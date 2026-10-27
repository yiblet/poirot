//! Observed prepared-source error replacement over a completed member mask.
use super::argument_role::ArgumentRole;
use super::broadcast::Broadcast;
use super::prepared::Prepared;
use super::with_demand::Deferred;
use crate::eval::host::{CallHost, SelectionAccess};
use crate::eval::limits::EvalResult;
use crate::eval::observe::OperationId;
use crate::eval::values::{FunctionResult, VALUE_ERROR, Value};
use crate::model::CellValue::Error;
use crate::model::{CellError, CellValue};

/// Catch policy and observation hooks independent of member traversal and geometry.
pub(in crate::eval::adapters) trait ErrorSelector {
    /// Decide whether this spreadsheet error requires the fallback source.
    fn catches(&self, error: CellError) -> bool;

    /// Publish an uncaught source check without evaluating the fallback.
    fn uncaught(
        &self,
        host: &mut CallHost<'_, '_>,
        source: Option<OperationId>,
        checked: &Value,
    ) -> EvalResult<()>;

    /// Prepare the demanded fallback while retaining its observation identity.
    fn fallback(
        &self,
        host: &mut CallHost<'_, '_>,
        source: Option<OperationId>,
        checked: &CellValue,
        role: &Deferred<Broadcast>,
    ) -> EvalResult<Prepared<Broadcast>>;
}

/// Preserve the observed source until every member has selected its recovery path.
/// Preparing an array expression computes that expression; reference fallback cells
/// are read only for selected output coordinates by the shared resolver.
pub(in crate::eval::adapters) fn execute<S: ErrorSelector>(
    host: &mut CallHost<'_, '_>,
    source: Deferred<Broadcast>,
    fallback: Deferred<Broadcast>,
    selector: S,
) -> FunctionResult<Value> {
    let (operation, value) = SelectionAccess::new(host).argument(source.argument())?;
    let source = Prepared::<Broadcast>::new(value);
    let shape = source.shape().ok_or(VALUE_ERROR)?;
    let count = shape
        .rows
        .checked_mul(shape.columns)
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(count)?;
    let mut mask = Vec::new();
    mask.try_reserve_exact(count)
        .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
    let allocation_target = host.target();
    let mut caught = None;
    let result = SelectionAccess::new(host).array(shape, |reader, row, column| {
        let value = source.read(reader, shape, row, column)?;
        let replace = matches!(value, Error(error) if selector.catches(error));
        if replace
            && caught.is_none()
            && let Error(error) = value
        {
            caught = Some(Error(error));
        }
        if mask.len() == mask.capacity() {
            mask.try_reserve(1)
                .map_err(|reservation_source| crate::eval::limits::EvaluationError::reservation(crate::eval::limits::CapabilityId::FunctionAdapter, Some(allocation_target), crate::eval::limits::EvaluationReasonError::AllocatingAFunctionResultOrScratchBuffer, reservation_source))?;
        }
        // SAFETY: the capacity check above reserves a slot whenever none remains.
        mask.push(replace);
        Ok(value)
    })?;
    let Some(checked) = caught else {
        let value = source.into_value();
        selector.uncaught(host, operation, &value)?;
        return Ok(value);
    };
    let prepared = selector.fallback(host, operation, &checked, &fallback)?;
    super::text::resolve_fallback(host, shape, result, &mask, Some(prepared))
}
