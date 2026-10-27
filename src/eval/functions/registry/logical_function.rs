/// Logical tests and conditional selection handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum LogicalFunction {
    /// ERROR.TYPE handler.
    ErrorType,
    /// IFS handler.
    Ifs,
    /// ISEVEN handler.
    IsEven,
    /// ISODD handler.
    IsOdd,
    /// SWITCH handler.
    Switch,

    /// ISREF handler.
    IsRef,
    /// IF handler.
    If,
    /// IFERROR handler.
    IfError,
    /// IFNA handler.
    IfNa,
    /// CHOOSE handler.
    Choose,
    /// AND handler.
    And,
    /// OR handler.
    Or,
    /// NOT handler.
    Not,
    /// TRUE handler.
    True,
    /// FALSE handler.
    False,
    /// Odd logical parity.
    Xor,
}
