/// Input categories accepted by a coercion rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedInput {
    /// Require present scalar.
    PresentScalar,
    /// Require existing number.
    ExistingNumber,
    /// Require existing boolean.
    ExistingBoolean,
    /// Require existing text.
    ExistingText,
    /// Require existing worksheet error.
    ExistingWorksheetError,
    /// Require finite decimal number.
    FiniteDecimalNumber,
    /// Require logical scalar.
    LogicalScalar,
    /// Require formula text.
    FormulaText,
    /// Require direct scalar provenance.
    Direct,
    /// Require reference scalar provenance.
    Reference,
    /// Require array member provenance.
    Array,
}
