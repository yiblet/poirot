use super::extension_failure::ExtensionFailure;

/// Result of a provider/host operation, preserving spreadsheet and operational failures.
pub type ExtensionResult<T> = Result<T, ExtensionFailure>;
