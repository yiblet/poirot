//! Save workbook and calculation state to native Poirot snapshots and restore it later.
//!
//! Use snapshots to resume application work, including authored edits and compatible
//! prepared calculation state. Use [`crate::Workbook::save_xlsx`] or
//! [`crate::Workbook::save_xls`] to create files for Excel.
pub use poirot_eval::artifact::*;
