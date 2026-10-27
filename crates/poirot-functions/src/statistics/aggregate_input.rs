/// Supply AGGREGATE with numeric values and a count of all nonblank values.
///
/// The caller applies reference and visibility rules and excludes errors when
/// requested. For numeric reductions, it propagates any source errors that remain.
/// COUNT ignores these errors; COUNTA counts them and empty text.
#[derive(Clone, Copy, Debug)]
pub struct AggregateInput<'a> {
    /// Numeric members only, excluding text, logical values and blank cells.
    pub numbers: &'a [f64],
    /// Count of nonblank values after filtering: numbers, logicals, text and errors.
    /// Empty text counts as nonblank.
    /// Must be at least `numbers.len()`; a smaller count yields #VALUE!.
    pub nonempty_count: usize,
}
