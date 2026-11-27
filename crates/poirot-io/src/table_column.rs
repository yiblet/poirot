/// A column's stable file identity and ordered header caption.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TableColumn {
    /// Positive column ID, local to the table.
    pub id: u32,
    /// Caption used to resolve structured references.
    pub name: String,
}
