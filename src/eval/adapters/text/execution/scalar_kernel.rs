use super::scalar::text_scalar;
use crate::eval::adapters::elementwise::{Broadcast, Kernel, MemberContext, Variadic};
use crate::eval::functions::registry::TextFunction;
use crate::eval::values::FunctionResult;
use crate::model::CellValue;

/// Adapts scalar text functions to shared rectangular execution.
/// It reads the current output coordinate's arguments in order before calling the selected text calculation.
pub(super) struct ScalarKernel(pub(super) TextFunction);

impl Kernel for ScalarKernel {
    type Signature = Variadic<Broadcast>;

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let values = arguments.scalar_values(context)?;
        text_scalar(context.host, self.0, &values)
    }
}
