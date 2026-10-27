//! Parse spreadsheet formulas and text values independently of workbook storage.
//! Use `formula::parse_formula` to build an expression tree; references remain
//! unresolved until a caller supplies workbook context. Parsing does not evaluate
//! formulas. Applications calculating workbooks should start with
//! [poirot::calculation](https://docs.rs/poirot/latest/poirot/calculation/).
//!
//! ```rust
//! use poirot_parsing::formula::{parse_formula, Expr};
//! let expression = parse_formula("=42")?;
//! assert!(matches!(expression, Expr::Number(n) if n == 42.0));
//! # Ok::<(), poirot_parsing::formula::ParseError>(())
//! ```
//!
//! | Module | Purpose |
//! | --- | --- |
//! | [`formula`] | Formula AST, printing and checked reference translation |
//! | [`criteria`] | Comparison prefixes and source-specific criterion operand syntax |
//! | [`number`] | Numeric input grammar with explicit separators and affixes |
//! | [`date`] | Borrowed DATEVALUE/TIMEVALUE date fields |
//! | [`datetime`] | Source-specific optional datetime syntax and unresolved fields |
//! | [`formatter`] | Pinned formatter datetime grammar with committed optional sections |
//! | [`java_number`] | Java hexadecimal floating literal grammar and binary64 rounding |
//!
//! The text parsers expose particular spreadsheet/source grammars, rather than one
//! universal date or number parser. Numeric input uses an explicit `NumberLocale`
//! and never consults the process locale. Date/time fields still require caller
//! calendar validation and default-year resolution. Use
//! [poirot-functions](https://docs.rs/poirot-functions) for spreadsheet coercion and
//! calendar algorithms built on these syntax modules.

pub mod criteria;
pub mod date;
mod date_syntax_error;
pub mod datetime;
pub mod formatter;
mod formatter_error;
pub mod formula;
pub mod java_number;
pub mod number;
mod number_error;
mod number_locale_error;
mod number_locale_token;
