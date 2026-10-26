//! Stored spreadsheet file metadata.
/// Author-supplied row hiding and filtering state, kept independently of cell presence.
/// File adapters currently do not retain these flags.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct RowVisibility {
    /// Row was explicitly hidden by the author.
    pub manually_hidden: bool,
    /// Row is excluded by filtering metadata.
    pub filtered_out: bool,
}
