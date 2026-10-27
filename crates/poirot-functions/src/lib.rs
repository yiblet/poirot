//! Workbook-independent spreadsheet algorithms, argument coercion, exact scalar
//! values and Excel display formatting. Use this crate when you supply operands and
//! execution policy yourself. For workbook loading, formula evaluation or incremental
//! scenarios, start with [poirot](https://docs.rs/poirot).
//!
//! ```rust
//! use poirot_functions::{math, scalar::CellValue};
//! use poirot_functions::format::{
//!     compile_format, render_value, FormatCompilePolicy, FormatRenderContext,
//! };
//!
//! let rounded = match math::round(12.345, 2.) {
//!     Ok(number) => CellValue::Number(number),
//!     Err(error) => CellValue::Error(error),
//! };
//! let plan = compile_format("0.00", FormatCompilePolicy::default())?;
//! let display = render_value(
//!     &rounded, &plan, FormatRenderContext::default(),
//! )?;
//! assert_eq!(display.text, "12.35");
//! # Ok::<(), poirot_functions::format::FormatCompileError>(())
//! ```
//!
//! | Task | Modules |
//! | --- | --- |
//! | Represent values and deferred arguments | [`scalar`] |
//! | Select argument conversions and retain operand origin | [`coercion`] |
//! | Compare spreadsheet numbers or operate on exact text units | [`numeric`], [`text_units`] |
//! | Compile number formats and render scalar values | [mod@format] |
//! | Match criteria and aggregate database rows | [`criteria`], [`database`] |
//! | Run a function algorithm on supplied operands | [`dates`], [`engineering`], [`financial`], [`logical`], [`lookup`], [`math`], [`matrix`], [`statistics`], [`text`] |
//!
//! These algorithms do not resolve cell references or own a workbook. Typed numeric
//! kernels accept already-admitted operands; family coercion APIs distinguish direct,
//! reference and array inputs. Hosts supply context such as calendar, locale, lazy
//! argument forcing and resource admission where required. Spreadsheet errors use
//! `scalar::CellError`; APIs with allocation, geometry or host failures expose
//! separate module-owned `thiserror` types, such as `lookup::LookupError` and
//! `statistics::StatisticalKernelError`. Match their variants and typed payloads;
//! their source chains retain dependency and host causes. Consult each function's contract for admitted inputs,
//! error precedence and compatibility policy.

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
