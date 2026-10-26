//! Parse spreadsheet formulas and text values.
//!
//! Use [`formula::parse_formula`] to build an expression tree. References remain
//! unresolved until the caller supplies a workbook context.
//!
//! ```rust
//! fn main() -> Result<(), poirot_parsing::formula::ParseError> {
//!     use poirot_parsing::formula::{parse_formula, Expr};
//!
//!     let expression = parse_formula("=42")?;
//!     assert!(matches!(expression, Expr::Number(n) if n == 42.0));
//!     Ok(())
//! }
//! ```
//!
//! The formula module also prints expressions and translates references for edits.
//! Other modules parse criteria, numbers, dates, and date/time inputs. Formula
//! calculation lives in [poirot::eval](https://docs.rs/poirot/latest/poirot/eval).

pub mod criteria;
pub mod date;
pub mod datetime;
pub mod formatter;
pub mod formula;
pub mod java_number;
pub mod number;
