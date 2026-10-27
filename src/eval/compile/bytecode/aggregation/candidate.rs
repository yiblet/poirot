/// Scoped identifier retained until a grouping call demands its aggregation role.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Candidate {
    /// Optional sheet qualifier used for defined-name lookup.
    pub sheet: Option<String>,
    /// Original identifier, never a text-valued aggregation shortcut.
    pub name: String,
}
