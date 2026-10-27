/// Stable identity for an explicitly unsupported execution capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CapabilityId {
    /// Formula syntax preparation and configured structural bounds.
    FormulaParsing,
    /// Optional observation or retained execution evidence.
    ExecutionObservation,
    /// Structural bytecode compilation before execution.
    FormulaCompilation,
    /// Identifier comparison outside the supported contract.
    IdentifierComparison,
    /// Unsupported builtin adapter behavior.
    FunctionAdapter,
    /// Unsupported numeric conversion behavior.
    NumericConversion,
    /// Unsupported date text parsing behavior.
    DateParsing,
    /// Unsupported value formatting behavior.
    Formatting,
    /// External data access.
    ExternalData,
    /// User-defined provider execution.
    UserDefinedFunction,
}
