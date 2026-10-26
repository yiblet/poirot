//! Exact binary64 identity; no floating-point equality or NaN canonicalization.
/// Exact binary64 identity for transport and cache diagnostics.
/// Equality preserves the sign of zero and each NaN payload; it is not spreadsheet numeric equality.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IeeeNumberBits(u64);
/// Classification of binary64 bits, separating signed zeros and subnormals from
/// ordinary finite values, infinities, and NaNs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IeeeNumberClass {
    /// Positive zero bit pattern.
    PositiveZero,
    /// Negative zero bit pattern.
    NegativeZero,
    /// Positive finite value below the normal exponent range.
    PositiveSubnormal,
    /// Negative finite value below the normal exponent range.
    NegativeSubnormal,
    /// Ordinary nonzero finite normal value.
    Finite,
    /// Positive infinity bit pattern.
    PositiveInfinity,
    /// Negative infinity bit pattern.
    NegativeInfinity,
    /// Any NaN bit pattern, without canonicalizing its payload.
    Nan,
}
impl IeeeNumberBits {
    /// Wrap exact binary64 bits without floating-point normalization.
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }
    /// Capture the number's exact binary64 representation.
    pub fn from_number(number: f64) -> Self {
        Self(number.to_bits())
    }
    /// Return the original binary64 payload.
    pub const fn bits(self) -> u64 {
        self.0
    }
    /// Reconstruct the number without changing its bits.
    pub fn number(self) -> f64 {
        f64::from_bits(self.0)
    }
    /// Classify bits without relying on floating-point equality.
    pub const fn class(self) -> IeeeNumberClass {
        let magnitude = self.0 & 0x7fff_ffff_ffff_ffff;
        let negative = self.0 >> 63 != 0;
        match magnitude {
            0 => {
                if negative {
                    IeeeNumberClass::NegativeZero
                } else {
                    IeeeNumberClass::PositiveZero
                }
            }
            1..=0x000f_ffff_ffff_ffff => {
                if negative {
                    IeeeNumberClass::NegativeSubnormal
                } else {
                    IeeeNumberClass::PositiveSubnormal
                }
            }
            0x7ff0_0000_0000_0000 => {
                if negative {
                    IeeeNumberClass::NegativeInfinity
                } else {
                    IeeeNumberClass::PositiveInfinity
                }
            }
            0x7ff0_0000_0000_0001.. => IeeeNumberClass::Nan,
            _ => IeeeNumberClass::Finite,
        }
    }
}
/// Unchecked exceptions thrown by inspected pinned handlers, not CellError values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KnownSourceException {
    /// Pinned numeric comparison rejects a special binary64 input.
    SpecialDoubleComparison {
        /// Source argument index that triggered the rejection.
        argument: u8,
        /// Exact rejected numeric payload.
        bits: IeeeNumberBits,
    },
    /// Pinned Commons Math iteration failed to converge.
    CommonsConvergence(CommonsConvergence),
    /// Pinned Poisson distribution constructor rejects a zero mean.
    PoissonMeanZeroConstructor,
    /// Pinned numeric handler throws while coercing an omitted argument.
    MissingArgumentCoercion,
    /// MultiOperandNumericFunction.collectValue rejects a RefListEval union.
    AggregateReferenceListConversion,
}
/// Reason a Commons Math iteration failed to converge in the reference algorithm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommonsConvergence {
    /// Source iteration limit was exhausted.
    IterationLimit,
    /// Continued fraction diverged to infinity.
    InfiniteContinuedFraction,
    /// Continued fraction produced NaN.
    NanContinuedFraction,
}
