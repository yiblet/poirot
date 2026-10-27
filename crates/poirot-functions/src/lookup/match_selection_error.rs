/// A malformed host position is distinct from a failure while reading a candidate.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum MatchSelectionError<E> {
    /// Preserve the host's original operational failure.
    #[error("could not read match candidate")]
    Reader(#[source] E),
    /// A reader returned a position outside the requested logical interval.
    #[error("match reader returned an invalid position")]
    InvalidPosition,
}
