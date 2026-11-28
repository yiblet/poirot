/// A stored calculated-column formula definition.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CalculatedColumnFormula {
    /// Formula text, independent of formulas stored in worksheet cells.
    pub text: String,
    /// Whether the file explicitly describes an array-style formula.
    pub array: Option<bool>,
}
