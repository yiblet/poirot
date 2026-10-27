//! Bounded CFB/BIFF admission before sparse XLS decoding.
//! Budgets are operational limits, not Excel validity rules.
//!
//! Covers CFB metadata capacities/chains, BIFF record inventory, per-traversal scan work and decoded cells,
//! row inventory, SST decoding and repeated text-cell clones. It does not estimate allocator overhead,
//! backend formula-reconstruction text/stack growth, number-format/name decoding,
//! or the sparse model's final allocation. Input byte limits remain separate.
//! Unknown BIFF records and inaccurate SST counts within budgets remain accepted.
//! Source contracts: POI SSTRecord and historical calamine0.36.1 CFB/XLS framing.
//! <https://github.com/tafia/calamine/tree/0a24c2a9f1e38c0932c1299e633270dc730db505/src>
//! Calamine credit is in root LICENSES/ACKNOWLEDGEMENTS; terms in LICENSES/MIT.txt.
//!
//! Depends on: nothing outside this folder.
mod admission;
mod biff_plan;
mod cfb;
mod checks;
mod globals_plan;
mod preflight_error;
mod record;
mod records;
mod resource_budget;
mod sheets;
mod sst_accounting;
mod substream;

pub(in crate::xls) use admission::admit;
pub(in crate::xls) use cfb::workbook_stream;
pub use preflight_error::PreflightError;
pub(in crate::xls) use record::Record;
#[cfg(test)]
pub(in crate::xls) use records::records;
pub use resource_budget::ResourceBudget;
pub(in crate::xls) use substream::Substream;
