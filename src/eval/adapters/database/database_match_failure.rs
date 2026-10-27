use crate::eval::values::FunctionFailure;
use poirot_functions::database::DatabaseError;

/// Preserve host aborts separately from database spreadsheet/domain failures.
pub(super) enum DatabaseMatchFailure {
    Domain(DatabaseError),
    Host(FunctionFailure),
}

impl From<DatabaseError> for DatabaseMatchFailure {
    fn from(error: DatabaseError) -> Self {
        Self::Domain(error)
    }
}

impl From<FunctionFailure> for DatabaseMatchFailure {
    fn from(error: FunctionFailure) -> Self {
        Self::Host(error)
    }
}
