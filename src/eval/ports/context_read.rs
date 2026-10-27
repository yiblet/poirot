/// Individual context fields consumed by executed evaluator operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextRead {
    /// Captured clock value.
    AsOf,
    /// Fixed local UTC offset.
    UtcOffset,
    /// Numeric slash-date component order.
    DateOrder,
    /// Workbook's 1900 or 1904 date system.
    DateSystem,
}
