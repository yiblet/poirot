/// A retained-result mutation could not preserve cache coordinates or accounting.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// Cell coordinates are outside the worksheet grid.
    #[error("invalid cached coordinate")]
    Coordinate(#[from] poirot_io::ModelError),
    /// Version sequence cannot advance further.
    #[error("cache generation overflow")]
    GenerationOverflow,
    /// Retained byte accounting cannot represent the replacement.
    #[error("cache retention size overflow")]
    RetentionOverflow,
    /// Existing result disappeared before replacing it.
    #[error("retained result disappeared before replacement")]
    MissingRetainedResult,
}
