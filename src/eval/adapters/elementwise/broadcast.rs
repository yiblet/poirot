use super::argument_role::ArgumentRole;
use super::member_context::MemberContext;
use crate::eval::host::ScalarOperand;
use crate::eval::values::FunctionResult;

/// Argument role whose shape contributes to the output rectangle and whose value is read at each output coordinate.
/// Singleton rows or columns are replicated along the corresponding output axis.
#[derive(Clone, Copy)]
pub(in crate::eval::adapters) struct Broadcast {
    pub(super) argument: usize,
    pub(super) member: usize,
}

impl ArgumentRole for Broadcast {
    const BROADCAST: bool = true;

    fn argument(&self) -> usize {
        self.argument
    }

    fn bind(argument: usize, member: usize, _present: bool) -> Self {
        Self { argument, member }
    }
}

impl Broadcast {
    /// Read a member without discarding its origin or explicit omission marker.
    #[inline]
    pub(in crate::eval::adapters) fn read(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
    ) -> FunctionResult<ScalarOperand> {
        context
            .lift
            .member(context.host, self.member, context.row, context.column)
    }

    /// Coerce a numeric member, applying a default only to explicit omission.
    #[inline]
    pub(in crate::eval::adapters) fn number(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        default: Option<f64>,
    ) -> FunctionResult<f64> {
        context.lift.number(
            context.host,
            self.member,
            context.row,
            context.column,
            default,
        )
    }
}
