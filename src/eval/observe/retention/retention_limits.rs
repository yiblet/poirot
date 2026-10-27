/// Limits on event count, copied scalar count, and physical text bytes in an execution trace.
#[derive(Clone, Copy, Debug)]
pub struct RetentionLimits {
    /// Maximum retained execution events.
    pub events: usize,
    /// Maximum scalar evidence values copied into the trace.
    pub scalar_values: usize,
    /// Maximum physical text bytes copied into retained evidence.
    pub text_bytes: usize,
}
