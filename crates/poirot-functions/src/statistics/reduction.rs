/// Ordered numeric reduction over values already admitted by the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericReduction {
    /// Sum starting at positive zero.
    Sum,
    /// Sum of squares starting at negative zero.
    SumSq,
    /// Arithmetic mean with an ordered sum starting at negative zero.
    Average,
    /// Smallest value, starting with the first admitted value.
    Minimum,
    /// Largest value, starting with the first admitted value.
    Maximum,
    /// Product starting at one, with an empty result of zero.
    Product,
}

/// Incremental numeric reduction in source order without retaining every operand.
/// The caller handles coercion, errors, and final worksheet normalization; intermediate values retain ordinary IEEE arithmetic.
/// The count saturates at u64::MAX, so callers requiring exact mean counts must stay within that bound.
#[derive(Clone, Copy, Debug)]
pub struct NumericReductionState {
    operation: NumericReduction,
    value: f64,
    count: u64,
}
impl NumericReductionState {
    /// Start an empty reduction with the operation's exact arithmetic identity.
    pub fn new(operation: NumericReduction) -> Self {
        Self {
            operation,
            value: match operation {
                NumericReduction::SumSq | NumericReduction::Average => -0.0,
                NumericReduction::Product => 1.0,
                _ => 0.0,
            },
            count: 0,
        }
    }

    /// Consume one admitted value in order, without early numeric normalization.
    pub fn push(&mut self, value: f64) {
        self.value = match self.operation {
            NumericReduction::Sum | NumericReduction::Average => self.value + value,
            NumericReduction::SumSq => self.value + value * value,
            NumericReduction::Minimum if self.count != 0 => self.value.min(value),
            NumericReduction::Maximum if self.count != 0 => self.value.max(value),
            NumericReduction::Minimum | NumericReduction::Maximum => value,
            NumericReduction::Product => self.value * value,
        };
        self.count = self.count.saturating_add(1);
    }

    /// Return raw arithmetic, or None for an empty arithmetic mean.
    pub fn finish(self) -> Option<f64> {
        match self.operation {
            NumericReduction::Average if self.count == 0 => None,
            NumericReduction::Average => Some(self.value / self.count as f64),
            NumericReduction::Product if self.count == 0 => Some(0.0),
            _ => Some(self.value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NumericReduction as R, NumericReductionState as S};

    fn reduced(operation: R, values: &[f64]) -> S {
        let mut state = S::new(operation);
        for value in values {
            state.push(*value);
        }
        state
    }

    #[test]
    fn exact_seeds_empty_results_and_signed_zero() {
        for operation in [R::Sum, R::Minimum, R::Maximum, R::Product] {
            assert_eq!(S::new(operation).finish().unwrap().to_bits(), 0);
        }
        assert_eq!(
            S::new(R::SumSq).finish().unwrap().to_bits(),
            (-0.0f64).to_bits()
        );
        assert_eq!(S::new(R::Average).finish(), None);
        for operation in [R::Average, R::Minimum, R::Maximum, R::Product] {
            assert_eq!(
                reduced(operation, &[-0.0]).finish().unwrap().to_bits(),
                (-0.0f64).to_bits()
            );
        }
        assert_eq!(reduced(R::Sum, &[-0.0]).finish().unwrap().to_bits(), 0);
        assert_eq!(reduced(R::SumSq, &[-0.0]).finish().unwrap().to_bits(), 0);
        // The primitive permits either sign for equal zeros. Singleton tests
        // above prove that an artificial zero seed does not replace the input.
        for operation in [R::Minimum, R::Maximum] {
            for values in [[-0.0, 0.0], [0.0, -0.0]] {
                assert_eq!(reduced(operation, &values).finish().unwrap(), 0.0);
            }
        }
        assert_eq!(reduced(R::Minimum, &[-4.0, -2.0]).finish(), Some(-4.0));
        assert_eq!(reduced(R::Maximum, &[-4.0, -2.0]).finish(), Some(-2.0));
    }

    #[test]
    fn copying_raw_state_preserves_sequential_arithmetic() {
        let mut sum = S::new(R::Sum);
        for (value, expected) in [(1e16f64, 1e16f64), (1.0, 1e16), (-1e16, 0.0), (2.0, 2.0)] {
            sum.push(value);
            assert_eq!(sum.finish().unwrap().to_bits(), expected.to_bits());
        }
        assert_eq!(
            reduced(R::Average, &[1e16, 1.0, -1e16, 2.0]).finish(),
            Some(0.5)
        );
        assert_eq!(reduced(R::SumSq, &[3.0, 4.0]).finish(), Some(25.0));
        assert_eq!(reduced(R::Product, &[2.0, 3.0, 4.0]).finish(), Some(24.0));
    }

    #[test]
    fn count_saturates_without_losing_the_accumulated_value() {
        let mut state = S::new(R::Average);
        state.count = u64::MAX;
        state.push(2.0);
        assert_eq!(state.count, u64::MAX);
        assert_eq!(state.finish(), Some(2.0 / u64::MAX as f64));
    }

    #[test]
    fn raw_nonfinite_subnormal_and_underflow_are_not_published_early() {
        let tiny = f64::from_bits(1);
        assert_eq!(
            reduced(R::Sum, &[tiny, tiny]).finish().unwrap().to_bits(),
            2
        );
        assert_eq!(
            reduced(R::Product, &[f64::MIN_POSITIVE, 0.5])
                .finish()
                .unwrap()
                .to_bits(),
            1u64 << 51
        );
        assert_eq!(
            reduced(R::Product, &[tiny, 0.5])
                .finish()
                .unwrap()
                .to_bits(),
            0
        );
        assert!(
            reduced(R::Product, &[f64::MAX, 2.0])
                .finish()
                .unwrap()
                .is_infinite()
        );
        assert!(
            reduced(R::Sum, &[f64::INFINITY, f64::NEG_INFINITY])
                .finish()
                .unwrap()
                .is_nan()
        );
    }
}
