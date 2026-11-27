/// Selects ordered columns of an Excel table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableColumns<T = String> {
    /// Every column of the table.
    All,
    /// One decoded column caption.
    One(T),
    /// Inclusive first and last decoded captions.
    Range(T, T),
}
