use super::executable::Executable;
use super::reference_bindings::ReferenceBindings;
use std::sync::Arc;

/// An executable shared by `Arc` plus reference coordinates for this formula.
/// Cloning shares the executable and copies instance bindings; mutable access
/// through `DerefMut` uses copy-on-write so other owners keep their program.
#[derive(Clone, Debug, PartialEq)]
pub struct Compiled {
    /// Shared immutable instructions and call bindings.
    pub executable: Arc<Executable>,
    /// Concrete reference coordinates for this formula instance.
    pub references: ReferenceBindings,
}

impl std::ops::Deref for Compiled {
    /// Shared executable instructions and call bindings, excluding concrete references.
    type Target = Executable;

    fn deref(&self) -> &Executable {
        &self.executable
    }
}

impl std::ops::DerefMut for Compiled {
    fn deref_mut(&mut self) -> &mut Executable {
        Arc::make_mut(&mut self.executable)
    }
}
