//! Reusable value and argument stacks shared by nested bytecode frames.

use crate::eval::host;
use crate::eval::values::Value;

/// Reusable value and argument stacks for bytecode execution within a run.
/// Nested calls retain their operands here so deferred adapters can revisit an
/// argument without executing it again.
#[derive(Default)]
pub(in crate::eval) struct Machine {
    pub(in crate::eval) values: Vec<Value>,
    pub(in crate::eval) operands: Vec<Option<host::HostOperand>>,
}

impl Machine {
    /// Whether no expression or deferred argument values remain active.
    #[cfg(test)]
    pub(in crate::eval) fn is_idle(&self) -> bool {
        self.values.is_empty() && self.operands.is_empty()
    }
}
