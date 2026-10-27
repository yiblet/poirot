/// Value inspection, conversion, and error construction handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum InformationFunction {
    /// ISFORMULA handler.
    IsFormula,
    /// ISERROR handler.
    IsError,
    /// ISERR handler.
    IsErr,
    /// ISNA handler.
    IsNa,
    /// ISNUMBER handler.
    IsNumber,
    /// ISTEXT handler.
    IsText,
    /// ISBLANK handler.
    IsBlank,
    /// ISLOGICAL handler.
    IsLogical,
    /// ISNONTEXT handler.
    IsNonText,
    /// TYPE handler.
    Type,
    /// N handler.
    N,
    /// T handler.
    T,
    /// NA handler.
    Na,
}
