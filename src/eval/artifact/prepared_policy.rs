/// Whether loading may reuse engine-prepared data or must recover only durable source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreparedPolicy {
    /// Reuse compatible state, otherwise return source and an explicit discard status.
    #[default]
    ReuseCompatible,
    /// Do not decode optional prepared data, even if its engine revision matches.
    /// Recovered source needs an explicit workspace policy; prepared policies are discarded.
    SourceOnly,
    /// Require present preparation matching this engine compatibility revision.
    RequireCompatible,
}
