//! Workbook-independent spreadsheet algorithms, scalar values, coercion, and display formatting.

/// Shared scalar values, exact text, errors, and deferred arguments.
pub mod scalar;

/// Explicit spreadsheet argument and result conversion policies.
pub mod coercion;

/// Spreadsheet number comparison and decimal conversion.
pub mod numeric;

/// Text counting, slicing, and case-insensitive comparisons.
pub mod text_units;

/// Compile Excel number formats and render stored scalar values.
pub mod format;

/// Criteria matching and aggregate selection.
pub mod criteria;

/// Database criteria and column aggregations.
pub mod database;

/// Calendar intervals and business-day calculations.
pub mod dates;

/// Engineering units, numeric representations, and special functions.
pub mod engineering;

/// Cash flows, interest, depreciation, and financial solvers.
pub mod financial;

/// Boolean reductions and conditional selection.
pub mod logical;

/// Reference lookup, matching, and search.
pub mod lookup;

/// Scalar arithmetic and mathematical functions.
pub mod math;

/// Array construction, selection, grouping, and matrix calculations.
pub mod matrix;

/// Statistical aggregates, distributions, and regression.
pub mod statistics;

/// Spreadsheet text functions and regular expressions.
pub mod text;
