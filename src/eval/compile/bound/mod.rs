//! Caller-owned bytecode plans with typed, owned inspection views.
//! Raw syntax remains generic and is retained for serialization/diagnostics.
//!
//! ```
//! use poirot::eval::{compile::bound::{FormulaPlan, Expression}, functions::registry::SearchFunction};
//! use poirot::model::{Workbook, CellAddress, CellValue};
//! let syntax = poirot_parsing::formula::parse_formula("VLOOKUP(2,{1,10;2,20},2,FALSE)")?;
//! let plan = FormulaPlan::bind(&syntax)?;
//! if let Expression::Search(search) = plan.expression() {
//!     assert_eq!(search.function, SearchFunction::VLookup);
//! }
//! let mut workbook = Workbook::new();
//! workbook.add_sheet("Sheet1")?;
//! assert_eq!(match poirot::eval::run_plan(&workbook, 0, CellAddress::new(0,0)?, &plan, Default::default(), Default::default()) { poirot::eval::EvaluationOutcome::Complete { value, .. } => value, outcome => panic!("evaluation aborted: {outcome:?}"), }, CellValue::Number(20.));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Depends on: functions (registry identities, extension resolution and retained
//! external names); sibling bytecode (compiled programs).

mod admission;
mod binding;
/// Structural failure while admitting caller-provided syntax.
mod binding_error;
mod domain;
/// Binding entry points from parsed syntax to bound plans.
mod entry;
/// Caller-owned syntax with its immutable bytecode.
mod formula_plan;
#[cfg(test)]
mod tests;

/// Structural admission failures for manually supplied syntax.
pub use binding_error::BindingError;

// Bound inspection expressions, family metadata, and bytecode-backed formula plans.
pub use domain::{
    Arguments, CriteriaExpr, DatabaseExpr, DateExpr, EngineeringExpr, Expression, FinancialExpr,
    InformationExpr, LogicalExpr, MathExpr, MatrixExpr, ReferenceExpr, SearchExpr, StatisticsExpr,
    TextExpr,
};
pub use formula_plan::FormulaPlan;

#[cfg(test)]
pub(in crate::eval) use entry::bind_with_executable;
#[cfg(test)]
pub(in crate::eval) use entry::{bind, bind_with_registry};
