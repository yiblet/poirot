//! Raw date serial calculations, separate from display formatting.
//! Calendar arithmetic uses a proleptic Gregorian calendar and explicitly models
//! Excel's 1900 leap-day slot. No machine-local timezone or locale is consulted.
//!
//! Depends on: host (CallHost), session (coercion), vm (deferred arguments), config
//! (calendar and clock), functions (identities and failures), limits (budgets) and values
//! (value channels).

mod business_days;
mod components;
mod day_count;
/// Date and time family dispatch.
mod dispatch;
mod modern;
/// Unit tests for date and time function execution.
#[cfg(test)]
mod tests;
/// Date and time text coercion stages.
mod text_coercion;

pub(in crate::eval) use dispatch::execute;
