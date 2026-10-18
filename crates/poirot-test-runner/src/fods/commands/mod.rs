//! Command handlers: argument validation, execution, and presentation.
/// Label known baseline gaps without changing pass/fail expectations.
pub mod classify;
/// Render source fixture metadata without evaluating it.
pub mod inspect;
/// Regenerate corpus artifacts from the pinned source checkout.
pub mod refresh;
/// Execute selected fixtures and report regressions and improvements.
pub mod run;
