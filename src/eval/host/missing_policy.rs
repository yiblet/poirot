/// Explicit treatment of omitted syntax during scalar selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::eval) enum MissingPolicy {
    /// Project omitted syntax as blank while retaining its missing flag.
    Blank,
    /// Test policy projecting omitted syntax as numeric zero.
    #[cfg(test)]
    Zero,
    /// Test policy rejecting omitted syntax.
    #[cfg(test)]
    Reject,
}
