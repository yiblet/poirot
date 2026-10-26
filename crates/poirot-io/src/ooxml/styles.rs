//! Checked direct cell-XF decoding and canonical style-table serialization.
mod codec;
mod read;
pub(super) mod write;
pub(crate) use read::{read_styles, validate_theme};
pub(crate) use write::StyleExport;
