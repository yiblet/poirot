use super::extension_error::ExtensionError;

/// Result of a provider/host operation, preserving spreadsheet and operational failures.
pub type ExtensionResult<T> = Result<T, ExtensionError>;
