/// A table filter expressed as selected literal values and an optional blank selector.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TableFilterValues {
    /// Explicit blank-value selection flag.
    pub blank: Option<bool>,
    /// Selected values, in file order and with their original lexical spellings.
    pub values: Vec<String>,
}
