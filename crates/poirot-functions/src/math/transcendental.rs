//! Operation order from pinned POI NumericFunction, not interchangeable identities.
//!
//! Java Math and Rust's platform math implementation need not produce identical
//! last bits on every target. These functions preserve POI's chosen operations;
//! the evaluator retains responsibility for rejecting non-finite results.

/// POI EXP computes Math.pow(Math.E, argument), using the rounded binary64 E.
pub fn exp(argument: f64) -> f64 {
    std::f64::consts::E.powf(argument)
}

/// POI LOG10 computes Math.log(argument) / Math.log(10), in that order.
pub fn log10(argument: f64) -> f64 {
    argument.ln() / 10.0_f64.ln()
}
