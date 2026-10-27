/// Date text does not match the selected field grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("date text does not match the selected field grammar")]
pub struct DateSyntaxError;
