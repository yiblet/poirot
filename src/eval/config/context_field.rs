/// A context category reported when a previously consumed setting changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::eval) enum ContextField {
    /// Captured clock changed.
    AsOf,
    /// Random seed changed.
    Seed,
    /// Fixed UTC offset changed.
    UtcOffset,
    /// Numeric date ordering changed.
    DateOrder,
    /// Workbook date system changed.
    DateSystem,
}
