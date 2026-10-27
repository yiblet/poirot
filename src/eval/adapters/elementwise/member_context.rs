use super::lift::Lift;
use crate::eval::host::CallHost;

/// Connects a kernel's current output coordinate to the shared broadcast plan and evaluator services.
/// Argument roles use it to read corresponding source members without owning traversal state.
pub(in crate::eval::adapters) struct MemberContext<'member, 'call, 'workbook> {
    /// Shared operational services; the family retains control over coercion order.
    pub(in crate::eval::adapters) host: &'member mut CallHost<'call, 'workbook>,
    pub(super) lift: &'member Lift,
    pub(super) row: usize,
    pub(super) column: usize,
}
