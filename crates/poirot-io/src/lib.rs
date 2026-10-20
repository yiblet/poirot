//! Read XLS and XLSX workbooks and write XLSX files.
//!
//! This crate converts workbook files to and from the shared data model. Imports
//! retain saved formula results and record unsupported or altered content in workbook
//! metadata. Writing preserves saved results; it does not calculate formulas.
//!
//! ```rust
//! use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
//! use poirot_model::Workbook;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut book = Workbook::new();
//!     book.add_sheet("Data")?.set_value("A1".parse()?, 42.)?;
//!     let bytes = write_xlsx_to_buffer(&book)?;
//!     let restored = read_workbook_bytes(&bytes)?;
//!     assert_eq!(restored.sheets().len(), 1);
//!     Ok(())
//! }
//! ```
//!
//! [`read_workbook`] reads a file; [`read_workbook_bytes`] reads an in-memory buffer.
//! Both detect XLS or XLSX from the contents and return the same workbook type.
//! [`ReadOptions`] configures import policies and limits.
//!
//! Legacy XLS output lives in [poirot-xls](https://docs.rs/poirot-xls).
//! Most applications should use [poirot](https://docs.rs/poirot), which provides the
//! same file operations alongside editing and calculation.
/// Checked BIFF fidelity recovery around the backend XLS decoder.
pub mod biff_overlay;
mod biff_preflight;
// Pre-decode BIFF/CFB resource admission and structural diagnostics.
pub use biff_preflight::{BiffPlan, PreflightError, ResourceBudget};
mod ooxml;
/// XLSX package identity, relationship, and external-link policy validation.
pub mod opc;

mod contract;
mod decoded_cells;
mod export_execution;
mod export_plan;
mod import_conversion;
mod numeric_representation;

// Import errors and read/write options.
pub use contract::{IoError, ReadOptions, WriteOptions};
// XLSX serialization to a complete byte buffer before file publication.
pub use export_execution::{
    write_xlsx, write_xlsx_to_buffer, write_xlsx_to_buffer_with_options, write_xlsx_with_options,
};
// Checked immutable XLSX export plans and selected feature/cache contracts.
pub use export_plan::{
    ExportFeatures, ExportFormat, ExportPlan, TypedCachePolicy, plan_xlsx_export,
};
// Signature-aware file and byte import with retained fidelity diagnostics.
pub use import_conversion::{
    read_workbook, read_workbook_bytes, read_workbook_bytes_with_options,
    read_workbook_with_options,
};
