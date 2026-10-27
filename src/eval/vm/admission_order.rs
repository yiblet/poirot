//! Ordering of builtin admission relative to policy dependency reads.

/// Observable ordering of invocation admission and policy dependency reads.
#[derive(Clone, Copy)]
pub(in crate::eval) enum AdmissionOrder {
    /// Tree/VM: a refused invocation consumes no policy or context reads.
    ObserverBeforePolicy,
    /// Public direct calls: profile/unknown lookup consumes policy before observation.
    PolicyBeforeObserver,
}
