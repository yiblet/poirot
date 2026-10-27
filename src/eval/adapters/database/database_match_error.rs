use crate::eval::values::FunctionError;
use poirot_functions::database::DatabaseError;

/// Preserve host aborts separately from database spreadsheet/domain failures.
#[derive(Debug, thiserror::Error)]
pub(super) enum DatabaseMatchError {
    /// Database semantic failure.
    #[error("database criterion rejected")]
    Domain(#[source] DatabaseError),
    /// Host coercion or resource rejection.
    #[error("database host operation rejected")]
    Host(#[source] FunctionError),
}

impl From<DatabaseError> for DatabaseMatchError {
    fn from(error: DatabaseError) -> Self {
        Self::Domain(error)
    }
}

impl From<FunctionError> for DatabaseMatchError {
    fn from(error: FunctionError) -> Self {
        Self::Host(error)
    }
}
