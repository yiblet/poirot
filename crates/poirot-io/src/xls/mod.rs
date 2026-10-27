//! Bounded BIFF8 (Excel 97-2003) XLS import: CFB/BIFF admission in
//! `preflight`, then sparse decoding into [`XlsWorkbook`].
//!
//! Layout references: [MS-XLS] record and Ptg structures
//! <https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xls/>
//! and pinned Apache POI StringPtg, NameRecord, AreaPtg, Ref3DPtg and
//! StringRecord serializers. Unknown formula token families keep the saved
//! value without formula source and produce a fidelity warning.
//!
//! The decoder is an internal import stage with no standalone public operations.
//! Use [`crate::read_workbook_bytes`] with [`crate::ReadOptions`] to import XLS;
//! fidelity omissions are reported through the returned workbook metadata.
//!
//! Depends on: the crate's cell, sheet and workbook-sink types.
mod cached_text;
mod cells;
mod formula;
mod preflight;
mod reconstruction;
mod result;
mod sheet_formula;
mod sst;
mod text;
mod xls_workbook;

pub use preflight::{PreflightError, ResourceBudget};
pub(crate) use xls_workbook::XlsWorkbook;
