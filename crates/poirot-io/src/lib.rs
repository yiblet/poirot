//! XLS/XLSX imports and checked XLSX export with resource and fidelity controls.
/// Checked BIFF fidelity recovery around the backend XLS decoder.
pub mod biff_overlay;
mod biff_preflight;
/// Pre-decode BIFF/CFB resource admission and structural diagnostics.
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

/// Typed import results and explicit read/write resource options.
pub use contract::{ImportReport, IoError, ReadOptions, WriteOptions};
/// XLSX serialization to a complete byte buffer before file publication.
pub use export_execution::{
    write_xlsx, write_xlsx_to_buffer, write_xlsx_to_buffer_with_options, write_xlsx_with_options,
};
/// Checked immutable XLSX export plans and selected feature/cache contracts.
pub use export_plan::{
    ExportFeatures, ExportFormat, ExportPlan, TypedCachePolicy, plan_xlsx_export,
};
/// Signature-aware file and byte import with retained fidelity diagnostics.
pub use import_conversion::{
    read_workbook, read_workbook_with_options, read_workbook_with_report, read_xls, read_xls_bytes,
    read_xls_bytes_with_budget, read_xls_bytes_with_options, read_xlsx, read_xlsx_bytes,
    read_xlsx_bytes_with_options, read_xlsx_bytes_with_policy,
};
