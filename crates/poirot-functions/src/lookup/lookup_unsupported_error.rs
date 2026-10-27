/// Explicit capability gaps reported without silently changing reference semantics.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum LookupUnsupportedError {
    /// Operation needs workbook or sheet metadata absent from the host.
    #[error("workbook metadata unavailable")]
    WorkbookMetadata,
    /// Host cannot resolve the computed reference.
    #[error("dynamic reference resolution unavailable")]
    DynamicReference,
    /// Host cannot supply arguments lazily.
    #[error("deferred arguments unavailable")]
    DeferredArguments,
    /// Lookup geometry is outside the supported vector contract.
    #[error("lookup requires a supported vector")]
    LookupNonVector,
    /// Result vector is too short for the selected lookup behavior.
    #[error("lookup result vector is too short")]
    LookupShortResult,
    /// Reference dimensions cannot represent the pinned source behavior.
    #[error("reference dimensions cannot represent source behavior")]
    SourceDimension,
    /// Scalar selection from this array has no supported coordinate contract.
    #[error("scalar array intersection unavailable")]
    ScalarArrayIntersection,
    /// Sheet-name control characters cannot be rendered with supported ADDRESS fidelity.
    #[error("address sheet name contains unsupported control characters")]
    AddressSheetNameControlCharacter,
}
