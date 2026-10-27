/// Criteria-based aggregation and subtotal handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CriteriaFunction {
    /// COUNTIF handler.
    CountIf,
    /// SUMIF handler.
    SumIf,
    /// AVERAGEIF handler.
    AverageIf,
    /// COUNTIFS handler.
    CountIfs,
    /// SUMIFS handler.
    SumIfs,
    /// AVERAGEIFS handler.
    AverageIfs,
    /// MAXIFS handler.
    MaxIfs,
    /// MINIFS handler.
    MinIfs,
    /// SUBTOTAL handler.
    Subtotal,
}
