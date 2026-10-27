use super::member_context::MemberContext;
use super::tuple_signature::Signature;
use crate::eval::host::CallHost;
use crate::eval::values::{FunctionResult, Value};
use crate::model::{CellAddress, CellValue};

/// A context-aware scalar kernel with explicit typed argument roles.
pub(in crate::eval::adapters) trait Kernel {
    /// Deferred argument roles supplying the kernel's scalar and collection inputs.
    type Signature: Signature;

    /// Prepare shared collections after geometry admission and before output traversal.
    fn prepare(
        &mut self,
        _host: &mut CallHost<'_, '_>,
        _arguments: &Self::Signature,
    ) -> FunctionResult<()> {
        Ok(())
    }

    /// Calculate a member, reading and coercing roles in the family's required order.
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue>;
}

/// Execute a typed kernel while keeping operational failures outside spreadsheet cells.
pub(in crate::eval::adapters) fn execute<K: Kernel>(
    host: &mut CallHost<'_, '_>,
    address: CellAddress,
    mut kernel: K,
) -> FunctionResult<Value> {
    let (lift, arguments) = K::Signature::admit(host)?;
    kernel.prepare(host, &arguments)?;
    lift.evaluate(host, address, |host, row, column| {
        kernel.apply(
            &mut MemberContext {
                host,
                lift: &lift,
                row,
                column,
            },
            &arguments,
        )
    })
}
