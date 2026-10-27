//! Stored spreadsheet file metadata.
/// Author-supplied row hiding and filtering state, kept independently of cell presence.
/// XLSX and XLS store one hidden flag per row: readers set `manually_hidden`, and the
/// XLSX writer saves either flag as a hidden row.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct RowVisibility {
    /// Row was explicitly hidden by the author.
    pub manually_hidden: bool,
    /// Row is excluded by filtering metadata.
    pub filtered_out: bool,
}
