//! Rust adaptations of Apache POI (Apache-2.0); class bindings are described below.
//! <https://github.com/apache/poi/tree/732120980140d5ed64b482c470e0b625cdb1ab15/poi/src/main/java/org/apache/poi/ss/formula/functions>
//! Pure numeric algorithms. The evaluator owns coercion, references and arity.
//! Source: pinned POI NumericFunction, MathX, ExcelArithmetic, Gcd, Lcm,
//! FactDouble, Quotient, CeilingMath/Precise, FloorMath/Precise and atp/MRound.
/// Source-compatible transcendental kernels shared by the math entry points.
pub mod transcendental;
use bigdecimal::{BigDecimal, RoundingMode, ToPrimitive};
use poirot_functions_core::CellError;
use std::str::FromStr;
/// Numeric kernel result; invalid domains and non-finite outputs become spreadsheet errors.
pub type MathResult = Result<f64, CellError>;
fn finite(n: f64) -> MathResult {
    if n.is_finite() {
        Ok(n)
    } else {
        Err(CellError::Num)
    }
}
fn inputs(ns: &[f64]) -> Result<(), CellError> {
    for &n in ns {
        finite(n)?;
    }
    Ok(())
}
/// Excel's 15-significant-digit decimal view, retaining exact short binary fractions.
pub fn approximate(n: f64) -> f64 {
    if !n.is_finite() || n == n.trunc() || n * 2048.0 == (n * 2048.0).trunc() {
        return n;
    }
    // TODO(coercion-non-argument): [LOW] Decimal normalization round-trips a numeric intermediate through 15-digit text and can discard binary64 precision.
    poirot_coercionv2::compatibility::coercev2_number_to_text(n)
        .parse()
        .unwrap_or(f64::NAN)
}
fn decimal(n: f64) -> Result<BigDecimal, CellError> {
    // TODO(coercion-non-argument): [LOW] Decimal normalization round-trips a numeric intermediate through 15-digit text and can discard binary64 precision.
    BigDecimal::from_str(&poirot_coercionv2::compatibility::coercev2_number_to_text(
        n,
    ))
    .map_err(|_| CellError::Num)
}
fn truncate(n: f64) -> f64 {
    approximate(n).trunc()
}
fn round_decimal(n: f64, digits: f64, mode: RoundingMode) -> MathResult {
    inputs(&[n, digits])?;
    let value = decimal(n)?;
    // Finite binary64 numbers need fewer than 400 decimal places. Bound both
    // scales locally before BigDecimal performs scale subtraction or allocates
    // powers of ten; huge user-provided digit counts must not allocate huge ints.
    if !(-400..=400).contains(&value.fractional_digit_count()) {
        return Err(CellError::Num);
    }
    let digits = digits.trunc();
    let rounded = if digits > 400.0 {
        value
    } else if digits < -400.0 {
        return if mode == RoundingMode::Up && value != 0 {
            Err(CellError::Num)
        } else {
            Ok(0.0)
        };
    } else {
        // SAFETY: Both scales are locally bounded to -400..=400, so scale
        // arithmetic cannot overflow and at most 800 powers of ten are needed.
        value.with_scale_round(digits as i64, mode)
    };
    // Parse the final decimal once: BigDecimal's ToPrimitive conversion scales
    // binary intermediates and introduces extra ULPs at extreme exponents.
    let result = finite(rounded.to_string().parse().map_err(|_| CellError::Num)?)?;
    // Spreadsheet decimal rounding has one zero, including negative inputs
    // rounded to zero; do not leak a binary signed zero into later comparisons.
    Ok(if result == 0.0 { 0.0 } else { result })
}
/// Round Excel's decimal view to the nearest place, with ties away from zero.
pub fn round(n: f64, digits: f64) -> MathResult {
    round_decimal(n, digits, RoundingMode::HalfUp)
}
/// Round Excel's decimal view away from zero at the requested decimal place.
pub fn round_up(n: f64, digits: f64) -> MathResult {
    round_decimal(n, digits, RoundingMode::Up)
}
/// Round Excel's decimal view toward zero; also implements TRUNC.
pub fn round_down(n: f64, digits: f64) -> MathResult {
    round_decimal(n, digits, RoundingMode::Down)
}
/// Compute inverse hyperbolic cosine with spreadsheet domain checks.
pub fn acosh(n: f64) -> MathResult {
    inputs(&[n])?;
    finite(((n * n - 1.0).sqrt() + n).ln())
}
/// Compute inverse hyperbolic sine.
pub fn asinh(n: f64) -> MathResult {
    inputs(&[n])?;
    finite(((n * n + 1.0).sqrt() + n).ln())
}
/// Compute inverse hyperbolic tangent with spreadsheet domain checks.
pub fn atanh(n: f64) -> MathResult {
    inputs(&[n])?;
    finite(((1.0 + n) / (1.0 - n)).ln() / 2.0)
}
/// Compute hyperbolic cosine with numeric error classification.
pub fn cosh(n: f64) -> MathResult {
    inputs(&[n])?;
    finite((std::f64::consts::E.powf(n) + std::f64::consts::E.powf(-n)) / 2.0)
}
/// Compute hyperbolic sine with numeric error classification.
pub fn sinh(n: f64) -> MathResult {
    inputs(&[n])?;
    finite((std::f64::consts::E.powf(n) - std::f64::consts::E.powf(-n)) / 2.0)
}
/// Compute hyperbolic tangent.
pub fn tanh(n: f64) -> MathResult {
    inputs(&[n])?;
    let a = std::f64::consts::E.powf(n);
    let b = std::f64::consts::E.powf(-n);
    finite((a - b) / (a + b))
}
/// Compute the square root of the input multiplied by pi.
pub fn sqrtpi(n: f64) -> MathResult {
    inputs(&[n])?;
    finite((n * std::f64::consts::PI).sqrt())
}
/// Compute the angle using Excel's x-then-y argument order.
pub fn atan2(x: f64, y: f64) -> MathResult {
    inputs(&[x, y])?;
    if x == 0.0 && y == 0.0 {
        Err(CellError::Div0)
    } else {
        finite(y.atan2(x))
    }
}
fn parity(n: f64, p: f64) -> MathResult {
    inputs(&[n])?;
    let mut m = approximate(n).abs().ceil();
    if m % 2.0 != p {
        m += 1.0;
    }
    finite(if n < 0.0 { -m } else { m })
}
/// Round away from zero to the next even integer.
pub fn even(n: f64) -> MathResult {
    parity(n, 0.0)
}
/// Round away from zero to the next odd integer.
pub fn odd(n: f64) -> MathResult {
    parity(n, 1.0)
}
/// Compute factorial after the function's integer/domain conversion.
pub fn fact(n: f64) -> MathResult {
    inputs(&[n])?;
    let n = truncate(n);
    if !(0.0..=170.0).contains(&n) {
        return Err(CellError::Num);
    }
    finite((1..=n as u32).fold(1.0, |a, b| a * b as f64))
}
/// Compute the product of alternate positive integers.
pub fn factdouble(n: f64) -> MathResult {
    inputs(&[n])?;
    let n = truncate(n);
    if !(0.0..=300.0).contains(&n) {
        return Err(CellError::Num);
    }
    let mut r = BigDecimal::from(1);
    let mut k = n as u32;
    while k > 0 {
        r *= BigDecimal::from(k);
        k = k.saturating_sub(2);
    }
    finite(r.to_f64().ok_or(CellError::Num)?)
}
/// Compute the number of unordered selections without repetition.
pub fn combin(n: f64, k: f64) -> MathResult {
    inputs(&[n, k])?;
    let n = truncate(n);
    let k = truncate(k);
    if n.abs() > i32::MAX as f64 || k.abs() > i32::MAX as f64 || k < 0.0 || n < k {
        return Err(CellError::Num);
    }
    let small = k.min(n - k) as u32;
    let large = k.max(n - k) as u32; // POI multiplies numerator first; preserve its overflow behavior.
    if small > 170 {
        return Err(CellError::Num);
    }
    let mut r = 1.0;
    for i in large..n as u32 {
        r *= (i + 1) as f64;
        if !r.is_finite() {
            return Err(CellError::Num);
        }
    }
    finite(r / fact(small as f64)?)
}
/// Exact integer binomial coefficient from complementary selection counts.
/// The recurrence divides at every step; intermediate numerator overflow cannot
/// reject a finite result. At most 1023 iterations are needed: when both counts
/// are at least 1024, C(a+b,a) >= 2^1024 exceeds finite binary64.
fn binomial_from_counts<E: From<CellError>>(
    left: bigdecimal::num_bigint::BigUint,
    right: bigdecimal::num_bigint::BigUint,
    visit: &mut impl FnMut() -> Result<(), E>,
) -> Result<f64, E> {
    use bigdecimal::num_bigint::BigUint;
    // Bound input limbs here so the intermediate-size proof is local.
    if left.bits() > 1024 || right.bits() > 1024 {
        return Err(CellError::Num.into());
    }
    let (small, large) = if left <= right {
        (left, right)
    } else {
        (right, left)
    };
    if small >= BigUint::from(1024u32) {
        return Err(CellError::Num.into());
    }
    let small = small.to_u32().ok_or(CellError::Num)?;
    let mut result = BigUint::from(1u32);
    for i in 1..=small {
        visit()?;
        result *= &large + i;
        // SAFETY: i comes from 1..=small, so the integer divisor is nonzero.
        result /= i;
        if result.bits() > 1024 {
            return Err(CellError::Num.into());
        }
    }
    finite(result.to_f64().ok_or(CellError::Num)?).map_err(Into::into)
}

/// Documented COMBINA domain: raw nonnegative operands truncated to n >= k.
/// Zero available kinds produce zero. Otherwise count selections with repetition
/// using C(n+k-1,k), retaining exact integer counts before the transformation.
/// Unlike the pinned POI COMBIN entry point, this uses bounded exact arithmetic.
pub fn combina(n: f64, k: f64) -> MathResult {
    combina_metered(n, k, &mut || Ok(()))
}
/// Count replacement combinations, admitting each exact-integer recurrence step.
/// Finite binary64 bounds scratch; no spreadsheet collection is constructed.
pub fn combina_metered<E: From<CellError>>(
    n: f64,
    k: f64,
    visit: &mut impl FnMut() -> Result<(), E>,
) -> Result<f64, E> {
    use bigdecimal::{FromPrimitive, num_bigint::BigUint};
    let (n, k) = poirot_coercionv2::combinations::coercev2_combina_counts(n, k)?;
    if n == 0.0 {
        return Ok(0.0);
    }
    // TODO(coercion-non-argument): [LOW] Exact binary64 integer counts are represented as arbitrary-precision integers for arithmetic; no scalar admission changes here.
    let n = BigUint::from_f64(n).ok_or(CellError::Num)?;
    // TODO(coercion-non-argument): [LOW] This representation change retains the admitted binary64 integer exactly.
    let k = BigUint::from_f64(k).ok_or(CellError::Num)?;
    let one = BigUint::from(1u32);
    if n < one {
        return Err(CellError::Num.into());
    }
    // SAFETY: the local comparison proves n >= one, so unsigned subtraction cannot underflow.
    let left = n - one;
    binomial_from_counts(left, k, visit)
}
/// Return the integer portion of a division.
pub fn quotient(n: f64, d: f64) -> MathResult {
    inputs(&[n, d])?;
    if d == 0.0 {
        return Err(CellError::Div0);
    }
    finite(truncate(finite(n / d)?))
}
fn scaled(n: f64, s: f64, round: RoundingMode) -> MathResult {
    inputs(&[n, s])?;
    if s == 0.0 {
        return Ok(0.0);
    }
    let step = decimal(s.abs())?;
    if step == 0 {
        return Err(CellError::Num);
    }
    // SAFETY: The local zero check returned above, so the decimal divisor is nonzero.
    let q = (decimal(n)? / &step).with_scale_round(0, round);
    finite((q * step).to_f64().ok_or(CellError::Num)?)
}
/// Round upward to a multiple of the absolute significance.
pub fn ceiling_precise(n: f64, s: f64) -> MathResult {
    scaled(n, s, RoundingMode::Ceiling)
}
/// Round downward to a multiple of the absolute significance.
pub fn floor_precise(n: f64, s: f64) -> MathResult {
    scaled(n, s, RoundingMode::Floor)
}
/// POI currently treats only negative mode as reversing negative-number rounding.
pub fn ceiling_math(n: f64, s: f64, mode: f64) -> MathResult {
    inputs(&[mode])?;
    scaled(
        n,
        s,
        if n < 0.0 && mode < 0.0 {
            RoundingMode::Floor
        } else {
            RoundingMode::Ceiling
        },
    )
}
/// Round to a significance using FLOOR.MATH's negative-number mode.
pub fn floor_math(n: f64, s: f64, mode: f64) -> MathResult {
    inputs(&[mode])?;
    scaled(
        n,
        s,
        if n < 0.0 && mode < 0.0 {
            RoundingMode::Ceiling
        } else {
            RoundingMode::Floor
        },
    )
}
/// Round to the nearest multiple with spreadsheet sign checks.
pub fn mround(n: f64, m: f64) -> MathResult {
    inputs(&[n, m])?;
    // MRound.evaluate tests the floating product, rather than comparing sign
    // bits. Opposite tiny operands can underflow to negative zero and pass.
    if n * m < 0.0 {
        return Err(CellError::Num);
    }
    scaled(n, m, RoundingMode::HalfUp)
}
fn gcd_integer(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}
const MAX_EXACT: f64 = 9007199254740992.0;
/// Compute the greatest common divisor of admitted integer operands.
pub fn gcd(ns: &[f64]) -> MathResult {
    if ns.is_empty() {
        return Err(CellError::Value);
    }
    let mut r = 0;
    for &n in ns {
        inputs(&[n])?;
        if !(0.0..=MAX_EXACT).contains(&n) {
            return Err(CellError::Num);
        }
        r = gcd_integer(r, truncate(n) as u64);
    }
    Ok(r as f64)
}
/// Compute the least common multiple of admitted integer operands.
pub fn lcm(ns: &[f64]) -> MathResult {
    if ns.is_empty() {
        return Err(CellError::Value);
    }
    // Lcm.evaluate has a separate single-argument path: it returns the
    // truncated double, without conversion to long or MAX_OUTPUT checking.
    if ns.len() == 1 {
        inputs(ns)?;
        // SAFETY: The enclosing branch requires ns.len() == 1 before reading element zero.
        return if ns[0] < 0.0 {
            Err(CellError::Num)
        } else {
            // SAFETY: The enclosing branch requires ns.len() == 1 before reading element zero.
            Ok(truncate(ns[0]))
        };
    }
    let mut r = 1u128;
    for (index, &n) in ns.iter().enumerate() {
        inputs(&[n])?;
        if n < 0.0 {
            return Err(CellError::Num);
        }
        // Java's double-to-long conversion saturates at Long.MAX_VALUE.
        let n = truncate(n) as i64 as u64;
        r = if r == 0 || n == 0 {
            0
        } else {
            (r / gcd_integer(r as u64, n) as u128) * n as u128
        };
        // Source checks the output only after combining a pair. In particular
        // an oversized first operand combined with zero still produces zero.
        if index > 0 && r > MAX_EXACT as u128 {
            return Err(CellError::Num);
        }
    }
    finite(r as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excel_arithmetic_decimal_view_and_exact_fast_paths() {
        // Pinned NumberToTextConverter rounds these extreme exponents differently
        // from a generic 15-digit scientific formatter.
        let tiny = f64::from_bits(0x0036319916D67853);
        assert_eq!(approximate(tiny), 1.2345678901235e-307);
        assert_eq!(
            decimal(tiny).unwrap(),
            BigDecimal::from_str("1.2345678901235e-307").unwrap()
        );
        assert_eq!(approximate(f64::from_bits(1)), 0.);
        assert_eq!(decimal(f64::from_bits(1)).unwrap(), BigDecimal::from(0));

        // approxValue preserves exact integers and short binary fractions, but
        // toBigDecimal always passes through NumberToTextConverter, even here.
        let integer = f64::from_bits(0x43E56A95319D63E1);
        assert_eq!(approximate(integer).to_bits(), integer.to_bits());
        assert_eq!(
            decimal(integer).unwrap(),
            BigDecimal::from_str("12345678901234600000").unwrap()
        );
        assert_eq!(approximate(1. / 2048.).to_bits(), (1f64 / 2048.).to_bits());
        assert_eq!(approximate(-0.).to_bits(), (-0f64).to_bits());
        assert_eq!(approximate(f64::INFINITY), f64::INFINITY);
        assert!(approximate(f64::NAN).is_nan());
    }
    #[test]
    fn inverse_hyperbolic_domains() {
        assert_eq!(acosh(0.5), Err(CellError::Num));
        assert_eq!(acosh(1.0), Ok(0.0));
        assert_eq!(atanh(1.0), Err(CellError::Num));
        assert_eq!(atanh(-1.0), Err(CellError::Num));
        assert_eq!(asinh(0.0), Ok(0.0));
        assert!((asinh(1.0).unwrap() - 0.881373587019543).abs() < 1e-14);
        assert_eq!(cosh(0.0), Ok(1.0));
        assert_eq!(sinh(0.0), Ok(0.0));
        assert_eq!(tanh(0.0), Ok(0.0));
        assert_eq!(sqrtpi(-1.0), Err(CellError::Num));
        assert!((sqrtpi(1.0).unwrap() - std::f64::consts::PI.sqrt()).abs() < 1e-15);
    }
    #[test]
    fn coordinates_and_parity() {
        assert_eq!(atan2(0.0, 0.0), Err(CellError::Div0));
        assert_eq!(atan2(0.0, 1.0), Ok(std::f64::consts::FRAC_PI_2));
        assert_eq!(odd(0.0), Ok(1.0));
        assert_eq!(odd(-1.2), Ok(-3.0));
        assert_eq!(even(-1.2), Ok(-2.0));
        assert_eq!(even(2.0000000000000004), Ok(2.0));
    }
    #[test]
    fn factorial_and_combination_boundaries() {
        assert_eq!(fact(-0.2), Ok(1.0));
        assert_eq!(fact(-1.0), Err(CellError::Num));
        assert_eq!(fact(171.0), Err(CellError::Num));
        assert_eq!(fact(2.9999999999999996), Ok(6.0));
        assert_eq!(factdouble(7.9), Ok(105.0));
        assert!(factdouble(300.0).unwrap().is_finite());
        assert_eq!(factdouble(301.0), Err(CellError::Num));
        assert_eq!(combin(5.9, 2.9), Ok(10.0));
        assert_eq!(combin(2.0, 3.0), Err(CellError::Num));
        assert_eq!(combin(1e10, 1.0), Err(CellError::Num));
    }
    #[test]
    fn decimal_rounding_and_modes() {
        assert_eq!(ceiling_precise(-4.3, -2.0), Ok(-4.0));
        assert_eq!(floor_precise(-4.3, -2.0), Ok(-6.0));
        assert_eq!(ceiling_math(-4.3, 2.0, -1.0), Ok(-6.0));
        assert_eq!(floor_math(-4.3, 2.0, -1.0), Ok(-4.0));
        assert_eq!(ceiling_math(5.0, 0.0, 0.0), Ok(0.0));
        assert_eq!(mround(-5.0, 2.0), Err(CellError::Num));
        assert_eq!(mround(0.7 + 0.1, 1.6), Ok(1.6));
        assert_eq!(mround(0.45, 0.1 * 3.0), Ok(0.6));
        assert_eq!(ceiling_precise(0.1 * 3.0, 0.1), Ok(0.3));
    }
    #[test]
    fn round_uses_decimal_ties_and_truncates_digit_counts() {
        // POI 5.4.1 MathX.round: NumberToTextConverter then BigDecimal HALF_UP.
        // LibreOffice 26.2.6.3 independently agrees on the computed near-half.
        for (n, digits, expected) in [
            (0.7 * 3.5, 1.0, 2.5),
            (-0.7 * 3.5, 1.0, -2.5),
            (2.4999999999999996, 0.0, 3.0),
            (-2.4999999999999996, 0.0, -3.0),
            (2.4999999999999, 0.0, 2.0),
            (-2.4999999999999, 0.0, -2.0),
            (249.99999999999997, -2.0, 300.0),
            (-249.99999999999997, -2.0, -300.0),
            (1.25, 1.9, 1.3),
            (125.0, -1.9, 130.0),
        ] {
            assert_eq!(round(n, digits), Ok(expected), "{n}, {digits}");
        }
    }
    #[test]
    fn directed_rounding_uses_decimal_values_and_distinct_modes() {
        // POI MathX UP/DOWN and NumericFunction.TRUNC share this decimal view.
        for sign in [-1.0, 1.0] {
            assert_eq!(round_up(sign * (0.1 * 3.0), 1.0), Ok(sign * 0.3));
            assert_eq!(round_down(sign * (0.3 * 3.0), 1.0), Ok(sign * 0.9));
            assert_eq!(round_up(sign * 1.21, 1.0), Ok(sign * 1.3));
            assert_eq!(round_down(sign * 1.29, 1.0), Ok(sign * 1.2));
            assert_eq!(round_up(sign * 121.0, -1.0), Ok(sign * 130.0));
            assert_eq!(round_down(sign * 129.0, -1.0), Ok(sign * 120.0));
        }
    }
    #[test]
    fn decimal_rounding_bounds_extreme_scales_without_float_scaling() {
        assert_eq!(round(1e300, 300.0), Ok(1e300));
        assert_eq!(round(1e-300, 300.0), Ok(1e-300));
        assert_eq!(round(1e308, -308.0), Ok(1e308));
        assert_eq!(round(f64::MAX, -308.0), Err(CellError::Num));
        assert_eq!(round(1e308, -309.0), Ok(0.0));
        assert_eq!(round_up(1.0, -309.0), Err(CellError::Num));
        assert_eq!(round(1.25, f64::MAX), Ok(1.25));
        assert_eq!(round(1.25, -f64::MAX), Ok(0.0));
        assert_eq!(round_up(-1.25, -f64::MAX), Err(CellError::Num));
        assert_eq!(round_down(-1.25, -f64::MAX), Ok(0.0));
        for f in [round, round_up, round_down] {
            assert_eq!(f(f64::INFINITY, 0.0), Err(CellError::Num));
            assert_eq!(f(1.0, f64::NAN), Err(CellError::Num));
            assert_eq!(f(-0.0, 0.0).unwrap().to_bits(), 0.0f64.to_bits());
            assert_eq!(f(0.0, -f64::MAX).unwrap().to_bits(), 0.0f64.to_bits());
        }
        assert_eq!(round(-0.01, 0.0).unwrap().to_bits(), 0.0f64.to_bits());
        assert_eq!(round_down(-0.01, 0.0).unwrap().to_bits(), 0.0f64.to_bits());
    }
    #[test]
    fn integral_arithmetic_and_errors() {
        assert_eq!(quotient(-7.0, 3.0), Ok(-2.0));
        assert_eq!(quotient(1.0, 0.0), Err(CellError::Div0));
        assert_eq!(quotient(880000000.0 * 0.00849, 3.0), Ok(2490400.0));
        assert_eq!(gcd(&[24.9, 18.1]), Ok(6.0));
        assert_eq!(gcd(&[0.0, 0.0]), Ok(0.0));
        assert_eq!(gcd(&[-0.1]), Err(CellError::Num));
        assert_eq!(lcm(&[6.0, 8.0]), Ok(24.0));
        assert_eq!(lcm(&[0.0, 8.0]), Ok(0.0));
        assert_eq!(lcm(&[9007199254740991.0, 2.0]), Err(CellError::Num));
        assert_eq!(gcd(&[]), Err(CellError::Value));
        assert_eq!(even(f64::INFINITY), Err(CellError::Num));
    }
    #[test]
    fn lcm_singleton_and_first_pair_follow_distinct_source_paths() {
        // Lcm.java: singleton returns ExcelArithmetic.truncate(d); its
        // multi-argument path casts to long and checks MAX_OUTPUT after pairs.
        assert_eq!(lcm(&[1e30]), Ok(1e30));
        assert_eq!(lcm(&[1e16, 0.0]), Ok(0.0));
        assert_eq!(lcm(&[1e30, 0.0]), Ok(0.0));
        assert_eq!(lcm(&[1e30, 7.0]), Err(CellError::Num));
        assert_eq!(lcm(&[1e16, 1.0, 0.0]), Err(CellError::Num));
        assert_eq!(lcm(&[-0.2]), Err(CellError::Num));
    }
    #[test]
    fn mround_sign_validation_preserves_product_underflow() {
        assert_eq!(mround(-1e-300, 1e-300), Ok(-1e-300));
        assert_eq!(mround(1e-300, -1e-300), Ok(1e-300));
        assert_eq!(mround(-1e-100, 1e-100), Err(CellError::Num));
    }
}

/// General unsigned positional result rendering.
pub mod radix;

#[cfg(test)]
mod replacement_combination_tests {
    use super::*;
    #[test]
    fn combina_uses_checked_replacement_combinations() {
        for (n, k, want) in [
            (4.23, 3., 20.),
            (10., 3., 220.),
            (3.2, 3.8, 10.),
            (0., 0., 0.),
            (1., 1., 1.),
            (1000., 2., 500500.),
        ] {
            assert_eq!(combina(n, k), Ok(want));
        }
        // Independent Python math.comb(1199,200) converted once to binary64.
        assert_eq!(combina(1000., 200.), Ok(1.667683306781366e233));
        // The source-pinned COMBIN contract intentionally retains its old limit.
        assert_eq!(combin(1199., 200.), Err(CellError::Num));
        assert_eq!(combina(1e308, 1.), Ok(1e308));
        assert_eq!(combina(1e308, 0.), Ok(1.));
        for (n, k) in [
            (1e308, 2.),
            (1e308, 1e308),
            (1000., 1000.),
            (-0.2, 0.),
            (2., -0.2),
            (1., 2.),
        ] {
            assert_eq!(combina(n, k), Err(CellError::Num));
        }
    }
    #[test]
    fn exact_primitive_obeys_pascal_symmetry_and_small_enumeration() {
        use bigdecimal::num_bigint::BigUint;
        let coefficient = |n: u32, k: u32| {
            binomial_from_counts::<CellError>(
                BigUint::from(k),
                BigUint::from(n - k),
                &mut || Ok(()),
            )
            .unwrap()
        };
        // Exhaustive subset enumeration supplies counts independently of the recurrence.
        for n in 1..=12u32 {
            for k in 0..=n {
                let expected = (0..(1u32 << n))
                    .filter(|bits| bits.count_ones() == k)
                    .count() as f64;
                assert_eq!(coefficient(n, k), expected);
                assert_eq!(coefficient(n, k), coefficient(n, n - k));
                if k > 0 && k < n {
                    assert_eq!(
                        coefficient(n, k),
                        coefficient(n - 1, k - 1) + coefficient(n - 1, k)
                    );
                }
            }
        }
        for n in 1..=10u32 {
            for k in 0..=n {
                assert_eq!(combina(n as f64, k as f64), Ok(coefficient(n + k - 1, k)));
            }
        }
    }
}

/// Direct power-series kernels.
pub mod series;
