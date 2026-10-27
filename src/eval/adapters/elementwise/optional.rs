use super::argument_role::ArgumentRole;

/// A role whose trailing source argument may be absent; explicit omissions and blanks remain present.
/// Keep this wrapper outermost, including around `WithDemand<R, D>`, so absence
/// can be checked before preparing a deferred source.
pub(in crate::eval::adapters) struct Optional<R>(
    pub(in crate::eval::adapters) Option<R>,
    pub(super) usize,
);

impl<R: ArgumentRole> ArgumentRole for Optional<R> {
    const BROADCAST: bool = R::BROADCAST;

    const EAGER: bool = R::EAGER;

    fn argument(&self) -> usize {
        self.1
    }

    fn bind(argument: usize, member: usize, present: bool) -> Self {
        Self(present.then(|| R::bind(argument, member, true)), argument)
    }
}
