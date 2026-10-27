use super::argument_role::ArgumentRole;
use super::lift::Lift;
use crate::eval::host::{CallHost, OperandReader};
use crate::eval::values::{FunctionResult, VALUE_ERROR};

/// A tuple of typed roles with shared eager broadcast admission.
pub(in crate::eval::adapters) trait Signature: Sized {
    /// Bind source ordinals without inspecting geometry or forcing arguments.
    fn bind(present: usize) -> Self;

    /// Admit eager geometry and bind roles without reading scalar or collection members.
    fn admit(host: &mut CallHost<'_, '_>) -> FunctionResult<(Lift, Self)>;
}

/// A signature whose eager source expressions can be prepared in source order.
pub(in crate::eval::adapters) trait PreparedSignature: Signature {
    /// Prepare eager operands while retaining lazy member reads and original roles.
    fn prepare(&self, host: &mut CallHost<'_, '_>) -> FunctionResult<()>;
}

macro_rules! signatures {
    ($($role:ident:$index:tt),+) => {
        impl<$($role: ArgumentRole),+> PreparedSignature for ($($role,)+) {
            fn prepare(&self, host: &mut CallHost<'_, '_>) -> FunctionResult<()> {
                $(if $index < host.len() && $role::EAGER { OperandReader::new(host).describe(self.$index.argument())?; })+
                Ok(())
            }
        }
        impl<$($role: ArgumentRole),+> Signature for ($($role,)+) {
            fn bind(present: usize) -> Self {
                let mut member = 0;
                let bound = ($({
                    let role = $role::bind($index, member, $index < present);
                    member += usize::from($role::BROADCAST && $role::EAGER && $index < present);
                    role
                },)+);
                let _ = member;
                bound
            }
            fn admit(host: &mut CallHost<'_, '_>) -> FunctionResult<(Lift, Self)> {
                let roles = [$($role::BROADCAST && $role::EAGER),+];
                let mut selected = [0usize; [$($index),+].len()];
                let mut count = 0;
                for (index, broadcast) in roles.into_iter().enumerate().take(host.len()) {
                    if broadcast {
                        // SAFETY: selected and roles have equal lengths; count cannot exceed the visited prefix.
                        selected[count] = index;
                        count += 1;
                    }
                }
                let indices = selected.get(..count).ok_or(VALUE_ERROR)?;
                let lift = if indices.iter().copied().eq(0..count) {
                    Lift::new(host, count)?
                } else {
                    Lift::new_selected(host, indices)?
                };
                let bound = Self::bind(host.len());
                Ok((lift, bound))
            }
        }
    };
}

signatures!(A:0);

signatures!(A:0, B:1);

signatures!(A:0, B:1, C:2);

signatures!(A:0, B:1, C:2, D:3);

signatures!(A:0, B:1, C:2, D:3, E:4);

signatures!(A:0, B:1, C:2, D:3, E:4, F:5);

signatures!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);

signatures!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);

signatures!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8);

signatures!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9);

signatures!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10);

signatures!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11);
