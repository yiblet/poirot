/// Let callers apply resource quotas before XML/XPath work and allocation.
/// Each call supplies a size or work estimate for the caller to check.
pub trait Host {
    /// Error returned when the caller refuses a resource request.
    type Error;

    /// Check an estimated work charge before the backend starts a phase it cannot interrupt.
    fn work(&mut self, units: u64) -> Result<(), Self::Error>;

    /// Check a text size in bytes, before allocation where possible.
    fn text(&mut self, bytes: usize) -> Result<(), Self::Error>;

    /// Check the number of tokens, nodes or output values that a collection can hold.
    fn values(&mut self, count: usize) -> Result<(), Self::Error>;
}
