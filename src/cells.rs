//! Text storage, text units, and shared-formula provenance.
//!
//! ```
//! use poirot::{CellAddress, cells::{SharedFormula, SharedFormulaId}};
//! let origin: CellAddress = "B2".parse()?;
//! let provenance = SharedFormula { group: SharedFormulaId(1), origin };
//! assert!(provenance.is_anchor(origin));
//! # Ok::<(), poirot::ModelError>(())
//! ```
pub use crate::model::{
    ExcelText, SharedFormula, SharedFormulaId, TextRef, TextUnits, TextUnitsError,
};
