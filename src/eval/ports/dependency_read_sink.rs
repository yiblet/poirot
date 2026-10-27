use super::read_event::ReadEvent;

/// Caller-owned consumption port. Implementations choose retention and persistence;
/// the evaluator does not allocate a second dependency/result cache.
pub trait DependencyReadSink {
    /// Accept a consumed read, retaining it according to the caller's policy.
    fn read(&mut self, event: ReadEvent);
}
