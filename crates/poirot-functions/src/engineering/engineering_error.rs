/// Spreadsheet value/domain failures from engineering conversions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EngineeringError {
    /// Input spelling or value type is invalid.
    #[error("input spelling or value type is invalid")]
    Value,
    /// Input or requested width exceeds the supported numeric range.
    #[error("input or requested width exceeds the supported numeric range")]
    Num,
}
