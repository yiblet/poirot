/// A source shape whose binding never reads a member or coerces a value.
pub(in crate::eval::adapters) trait ArgumentRole: Sized {
    /// Whether this role contributes geometry to rectangular lifting.
    const BROADCAST: bool;

    /// Whether geometry may be inspected before a selector demands this argument.
    const EAGER: bool = true;

    /// Return the original source ordinal without resolving it.
    fn argument(&self) -> usize;

    /// Bind the source slot and its position among broadcast operands.
    fn bind(argument: usize, member: usize, present: bool) -> Self;
}
