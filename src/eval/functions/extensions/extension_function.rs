use super::extension_host::ExtensionHost;
use super::extension_output::ExtensionOutput;
use super::extension_result::ExtensionResult;

/// A registered formula implementation called through a budgeted [`ExtensionHost`].
/// Closures implementing the matching signature can be registered directly. Charge
/// provider-owned work through the host; do not use ambient clock/random/workbook state
/// that is absent from the declared cacheability contract and implementation revision.
pub trait ExtensionFunction: Send + Sync {
    /// Execute the provider through its declared, budgeted host capabilities.
    fn evaluate(&self, host: &mut dyn ExtensionHost) -> ExtensionResult<ExtensionOutput>;
}

impl<F> ExtensionFunction for F
where
    F: Fn(&mut dyn ExtensionHost) -> ExtensionResult<ExtensionOutput> + Send + Sync,
{
    fn evaluate(&self, host: &mut dyn ExtensionHost) -> ExtensionResult<ExtensionOutput> {
        self(host)
    }
}
