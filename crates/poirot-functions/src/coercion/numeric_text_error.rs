/// Catchable numeric-text input failures.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum NumericTextError {
    /// Invalid numeric text input.
    #[error("invalid numeric text argument")]
    Value,
    /// Numeric input is outside its supported domain.
    #[error("numeric text is outside the supported domain")]
    Num,
}
