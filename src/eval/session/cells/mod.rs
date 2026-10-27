//! One cell read's lifecycle: source resolution, memo and cache reuse, recursive
//! execution and completion delivery, plus prepared formula execution at a target.
//!
//! Depends on: the parent session state and its reads and projection executors, vm
//! (formula execution), spill (spill resolution and admission), reuse (retained results
//! and read recording), ports (observer ports), inputs (effective workbook), compile
//! (prepared formulas), config (execution policies), coordinates, limits and values.

/// Cell bookkeeping helpers shared by the lifecycle stages.
mod bookkeeping;
/// One cell read attempt and its stages.
mod cell_attempt;
/// The cell read executor.
mod cell_evaluator;
/// A finished attempt ready for completion delivery.
mod cell_read;
/// The reuse decision for one cell read.
mod cell_reuse;
/// Borrowed completion delivery state.
mod completion_context;
/// The prepared and bound formula executor.
mod formula_execution;
/// Authored content resolved through scenario overrides.
mod resolved_source;
#[cfg(test)]
mod tests;

pub(in crate::eval) use cell_evaluator::CellEvaluator;
pub(in crate::eval) use formula_execution::FormulaExecution;
pub(in crate::eval) use resolved_source::ResolvedSource;
#[cfg(test)]
pub(in crate::eval) use resolved_source::SOURCE_RESOLUTIONS;
