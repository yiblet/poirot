//! Observed prepared-source error replacement over a completed member mask.
use super::{ArgumentRole, Broadcast, Deferred, Prepared};
use crate::execution_observation::OperationId;
use crate::host::CallHost;
use crate::*;

/// Catch policy and observation hooks independent of member traversal and geometry.
pub(in crate::adapters) trait ErrorSelector {
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
pub(in crate::adapters) fn execute<S: ErrorSelector>(
    host: &mut CallHost<'_, '_>,
    source: Deferred<Broadcast>,
    fallback: Deferred<Broadcast>,
    selector: S,
) -> FunctionResult<Value> {
    let (operation, value) = host.selected_argument(source.argument())?;
    let source = Prepared::<Broadcast>::new(value);
    let shape = source.shape(host).ok_or(VALUE_ERROR)?;
    let count = shape
        .rows
        .checked_mul(shape.columns)
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(count)?;
    let mut mask = Vec::new();
    mask.try_reserve_exact(count)
        .map_err(|_| host.allocation_failure())?;
    let allocation_failure = host.allocation_failure();
    let mut caught = None;
    let result = host.selected_array(shape, |reader, row, column| {
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
                .map_err(|_| allocation_failure.clone())?;
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
