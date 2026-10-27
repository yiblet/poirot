//! Bound expression records: the executable formula tree and one call record per
//! builtin family, with read-only accessors.
//!
//! Depends on: functions (registry function identities and extension bindings).

/// Deferred call operands with source and lookup spellings.
mod arguments;
/// Bound criteria call.
mod criteria_expr;
/// Bound database call.
mod database_expr;
/// Bound date call.
mod date_expr;
/// Bound engineering call.
mod engineering_expr;
/// Executable formula tree with resolved handler identities.
mod expression;
/// Bound financial call.
mod financial_expr;
/// Bound information call.
mod information_expr;
/// Bound logical call.
mod logical_expr;
/// Bound math call.
mod math_expr;
/// Bound matrix call.
mod matrix_expr;
/// Bound reference call.
mod reference_expr;
/// Bound search call.
mod search_expr;
/// Bound statistics call.
mod statistics_expr;
/// Bound text call.
mod text_expr;

pub use arguments::Arguments;
pub use criteria_expr::CriteriaExpr;
pub use database_expr::DatabaseExpr;
pub use date_expr::DateExpr;
pub use engineering_expr::EngineeringExpr;
pub use expression::Expression;
pub use financial_expr::FinancialExpr;
pub use information_expr::InformationExpr;
pub use logical_expr::LogicalExpr;
pub use math_expr::MathExpr;
pub use matrix_expr::MatrixExpr;
pub use reference_expr::ReferenceExpr;
pub use search_expr::SearchExpr;
pub use statistics_expr::StatisticsExpr;
pub use text_expr::TextExpr;
