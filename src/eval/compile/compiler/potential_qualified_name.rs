/// A sheet-qualified defined name found while preparing a formula.
/// It is retained in potential-dependency metadata even when execution later skips
/// the branch containing the lookup.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PotentialQualifiedName {
    /// Sheet qualifier as written in the formula.
    pub sheet: String,
    /// Defined-name spelling.
    pub name: String,
}
