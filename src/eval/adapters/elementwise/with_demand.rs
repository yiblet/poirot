use super::argument_role::ArgumentRole;
use super::demand::{Demand, Eager, Lazy};
use super::prepared::Prepared;
use crate::eval::host::{CallHost, SelectionAccess};
use crate::eval::limits::EvalResult;

/// Changes when an argument role may be inspected without changing its scalar, collection, or broadcast shape.
/// Use `Optional<WithDemand<R, D>>` for a trailing optional role; wrapping `Optional` in demand or nesting demand wrappers is unsupported.
pub(in crate::eval::adapters) struct WithDemand<R, D = Eager> {
    pub(super) role: R,
    pub(super) demand: std::marker::PhantomData<D>,
}

/// A shape role whose source is prepared only after selection.
pub(in crate::eval::adapters) type Deferred<R> = WithDemand<R, Lazy>;

impl<R: ArgumentRole, D: Demand> ArgumentRole for WithDemand<R, D> {
    const BROADCAST: bool = R::BROADCAST;

    const EAGER: bool = D::EAGER;

    fn argument(&self) -> usize {
        self.role.argument()
    }

    fn bind(argument: usize, member: usize, present: bool) -> Self {
        Self {
            role: R::bind(argument, member, present),
            demand: std::marker::PhantomData,
        }
    }
}

impl<R> WithDemand<R, Eager> {
    /// Access the eager shape's ordinary scalar, collection, or member operations.
    pub(in crate::eval::adapters) fn role(&self) -> &R {
        &self.role
    }
}

impl<R: ArgumentRole> WithDemand<R, Lazy> {
    /// Transfer a demanded value once, retaining its original reference/array channel.
    /// The executor owns it through all later shape inspection and member reads.
    pub(in crate::eval::adapters) fn prepare(
        &self,
        host: &mut CallHost<'_, '_>,
    ) -> EvalResult<Prepared<R>> {
        Ok(Prepared::new(
            SelectionAccess::new(host).argument(self.argument())?.1,
        ))
    }
}
