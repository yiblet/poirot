//! Unit tests for workbook, cell storage, style and shared-formula contracts.

/// Column query counters over row pages and stored slots.
mod column_counters;
/// Typed validation failures and preserved causes.
mod error_contracts;
/// Owned formula normalization and empty-source rejection.
mod formula_normalization;
/// Sheet and defined-name identifier lookup policy.
mod identifier_consumers;
/// Formula cache and source separation for scalar text.
mod scalar_boundary;
/// Shared-formula provenance through edits.
mod shared_formulas;
/// Sheet reordering and duplication.
mod sheet_management;
/// Authored visual styles, sharing and snapshot restore.
mod visual_styles;
/// Addresses, sheet names, formulas, errors and name scopes.
mod workbook_basics;

use super::*;
