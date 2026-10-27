/// The observation responsible for an unpublished-footprint dependency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelfRead {
    Blank,
    Sparse,
}
