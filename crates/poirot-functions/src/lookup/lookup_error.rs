use crate::lookup::{LookupOperationalError, LookupUnsupportedError, LookupValueError};

/// Lookup failure preserving spreadsheet errors, operational aborts, and unsupported capabilities.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum LookupError {
    /// Catchable error returned as a spreadsheet value.
    #[error("lookup returned a spreadsheet error")]
    Spreadsheet(#[source] LookupValueError),
    /// Host execution failure that formula error handlers cannot catch.
    #[error("lookup execution failed")]
    Operational(#[source] LookupOperationalError),
    /// Required lookup/reference capability is unavailable.
    #[error("lookup capability is unavailable")]
    Unsupported(#[source] LookupUnsupportedError),
}

impl From<LookupValueError> for LookupError {
    fn from(error: LookupValueError) -> Self {
        Self::Spreadsheet(error)
    }
}

#[cfg(test)]
mod tests {
    use super::LookupError;
    use crate::lookup::{LookupOperationalError, LookupUnsupportedError, LookupValueError};
    use std::error::Error;

    #[test]
    fn classified_failures_preserve_their_typed_causes() {
        let failures = [
            LookupError::Spreadsheet(LookupValueError::Ref),
            LookupError::Operational(LookupOperationalError::Cancelled),
            LookupError::Unsupported(LookupUnsupportedError::DynamicReference),
        ];
        assert_eq!(
            failures[0]
                .source()
                .and_then(|source| source.downcast_ref::<LookupValueError>()),
            Some(&LookupValueError::Ref),
        );
        assert_eq!(
            failures[1]
                .source()
                .and_then(|source| source.downcast_ref::<LookupOperationalError>()),
            Some(&LookupOperationalError::Cancelled),
        );
        assert_eq!(
            failures[2]
                .source()
                .and_then(|source| source.downcast_ref::<LookupUnsupportedError>()),
            Some(&LookupUnsupportedError::DynamicReference),
        );
        fn assert_send_sync_static<T: Send + Sync + 'static>() {}
        assert_send_sync_static::<LookupError>();
    }
}
