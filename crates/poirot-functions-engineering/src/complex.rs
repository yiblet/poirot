//! Independently derived Cartesian complex arithmetic for spreadsheet functions.
//! Principal branches follow https://dlmf.nist.gov/4.2; trigonometric identities
//! follow https://dlmf.nist.gov/4.21. No external implementation was reused.
use poirot_coercionv2::{complex::ComplexNumber, CellError};
use std::f64::consts::{LN_10, LN_2, PI};

/// Unary complex operations whose results retain the input's imaginary unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unary {
    /// Reflect across the real axis.
    Conjugate,
    /// Complex cosine.
    Cos,
    /// Complex hyperbolic cosine.
    Cosh,
    /// Complex cotangent.
    Cot,
    /// Complex cosecant.
    Csc,
    /// Complex hyperbolic cosecant.
    Csch,
    /// Complex exponential.
    Exp,
    /// Principal natural logarithm.
    Ln,
    /// Principal base-ten logarithm.
    Log10,
    /// Principal base-two logarithm.
    Log2,
    /// Complex secant.
    Sec,
    /// Complex hyperbolic secant.
    Sech,
    /// Complex sine.
    Sin,
    /// Complex hyperbolic sine.
    Sinh,
    /// Principal square root.
    Sqrt,
    /// Complex tangent.
    Tan,
}
fn finite(z: ComplexNumber) -> Result<ComplexNumber, CellError> {
    if z.real.is_finite() && z.imaginary.is_finite() {
        Ok(z)
    } else {
        Err(CellError::Num)
    }
}
fn parts(z: ComplexNumber, real: f64, imaginary: f64) -> Result<ComplexNumber, CellError> {
    finite(ComplexNumber {
        real,
        imaginary,
        suffix: z.suffix,
    })
}
/// Compute the modulus without overflowing intermediate coefficient squares.
pub fn magnitude(z: ComplexNumber) -> Result<f64, CellError> {
    finite(z)?;
    let value = z.real.hypot(z.imaginary);
    if value.is_finite() {
        Ok(value)
    } else {
        Err(CellError::Num)
    }
}
/// Compute the principal angle in (-pi, pi], ignoring the spelling of signed zero.
pub fn argument(z: ComplexNumber) -> Result<f64, CellError> {
    finite(z)?;
    if z.real == 0.0 && z.imaginary == 0.0 {
        return Err(CellError::Div0);
    }
    Ok(if z.imaginary == 0.0 && z.real < 0.0 {
        PI
    } else {
        z.imaginary.atan2(z.real)
    })
}
/// Add coefficients without rendering intermediate complex text.
pub fn add(a: ComplexNumber, b: ComplexNumber) -> Result<ComplexNumber, CellError> {
    finite(a)?;
    finite(b)?;
    parts(a, a.real + b.real, a.imaginary + b.imaginary)
}
/// Subtract coefficients without rendering intermediate complex text.
pub fn subtract(a: ComplexNumber, b: ComplexNumber) -> Result<ComplexNumber, CellError> {
    finite(a)?;
    finite(b)?;
    parts(a, a.real - b.real, a.imaginary - b.imaginary)
}
// Represent each product independently before aligning exponents. Scaling both
// numerator coefficients together would erase a small, still relevant component.
fn product(a: f64, b: f64) -> (f64, i32) {
    if a == 0.0 || b == 0.0 {
        return (a * b, 0);
    }
    let (a, ae) = libm::frexp(a);
    let (b, be) = libm::frexp(b);
    (a * b, ae.saturating_add(be))
}
fn dot(a: f64, b: f64, c: f64, d: f64) -> (f64, i32) {
    let (first, fe) = product(a, b);
    let (second, se) = product(c, d);
    if first == 0.0 {
        return (second, se);
    }
    if second == 0.0 {
        return (first, fe);
    }
    let exponent = fe.max(se);
    (
        libm::scalbn(first, fe.saturating_sub(exponent))
            + libm::scalbn(second, se.saturating_sub(exponent)),
        exponent,
    )
}
/// Multiply with scaled component products, preserving finite cancellation results.
pub fn multiply(a: ComplexNumber, b: ComplexNumber) -> Result<ComplexNumber, CellError> {
    finite(a)?;
    finite(b)?;
    let (r, re) = dot(a.real, b.real, -a.imaginary, b.imaginary);
    let (i, ie) = dot(a.real, b.imaginary, a.imaginary, b.real);
    parts(a, libm::scalbn(r, re), libm::scalbn(i, ie))
}
/// Divide using independently scaled component sums and a normalized denominator.
pub fn divide(a: ComplexNumber, b: ComplexNumber) -> Result<ComplexNumber, CellError> {
    finite(a)?;
    finite(b)?;
    let maximum = b.real.abs().max(b.imaginary.abs());
    if maximum == 0.0 {
        return Err(CellError::Num);
    }
    let (_, exponent) = libm::frexp(maximum);
    let real = libm::scalbn(b.real, exponent.saturating_neg());
    let imaginary = libm::scalbn(b.imaginary, exponent.saturating_neg());
    let denominator = real * real + imaginary * imaginary;
    let (r, re) = dot(a.real, b.real, a.imaginary, b.imaginary);
    let (i, ie) = dot(a.imaginary, b.real, -a.real, b.imaginary);
    parts(
        a,
        libm::scalbn(
            r / denominator,
            re.saturating_sub(exponent.saturating_mul(2)),
        ),
        libm::scalbn(
            i / denominator,
            ie.saturating_sub(exponent.saturating_mul(2)),
        ),
    )
}
fn log_magnitude(z: ComplexNumber) -> Result<f64, CellError> {
    let maximum = z.real.abs().max(z.imaginary.abs());
    if maximum == 0.0 {
        return Err(CellError::Num);
    }
    let ratio = z.real.abs().min(z.imaginary.abs()) / maximum;
    Ok(maximum.ln() + (ratio * ratio).ln_1p() / 2.0)
}
fn sqrt(z: ComplexNumber) -> Result<ComplexNumber, CellError> {
    let maximum = z.real.abs().max(z.imaginary.abs());
    if maximum == 0.0 {
        return parts(z, 0.0, 0.0);
    }
    let x = z.real / maximum;
    let y = z.imaginary / maximum;
    let larger = maximum.sqrt() * ((x.hypot(y) + x.abs()) / 2.0).sqrt();
    if z.real >= 0.0 {
        parts(z, larger, z.imaginary / (2.0 * larger))
    } else {
        parts(
            z,
            z.imaginary.abs() / (2.0 * larger),
            if z.imaginary < 0.0 { -larger } else { larger },
        )
    }
}
// Multiplication by an exact zero must stay zero even if the exponential itself
// overflows. Logarithmic rescaling handles the remaining representable products.
fn exp_times(exponent: f64, coefficient: f64) -> f64 {
    if coefficient == 0.0 {
        return coefficient;
    }
    let exponential = exponent.exp();
    if exponential.is_finite() {
        exponential * coefficient
    } else {
        (exponent + coefficient.abs().ln())
            .exp()
            .copysign(coefficient)
    }
}
fn hyperbolic_times(x: f64, coefficient: f64, odd: bool) -> f64 {
    let magnitude = x.abs();
    let correction = if odd {
        -(-2.0 * magnitude).exp_m1()
    } else {
        1.0 + (-2.0 * magnitude).exp()
    };
    let result = exp_times(magnitude - LN_2, coefficient * correction);
    if odd {
        result * x.signum()
    } else {
        result
    }
}
fn sin_cos(z: ComplexNumber, cosine: bool) -> Result<ComplexNumber, CellError> {
    let (sine, cos) = z.real.sin_cos();
    if cosine {
        parts(
            z,
            hyperbolic_times(z.imaginary, cos, false),
            -hyperbolic_times(z.imaginary, sine, true),
        )
    } else {
        parts(
            z,
            hyperbolic_times(z.imaginary, sine, false),
            hyperbolic_times(z.imaginary, cos, true),
        )
    }
}
fn reciprocal(z: ComplexNumber) -> Result<ComplexNumber, CellError> {
    divide(
        ComplexNumber {
            real: 1.0,
            imaginary: 0.0,
            suffix: z.suffix,
        },
        z,
    )
}
fn reciprocal_trig(z: ComplexNumber, cosine: bool) -> Result<ComplexNumber, CellError> {
    if z.imaginary.abs() <= 1.0 {
        return reciprocal(sin_cos(z, cosine)?);
    }
    let (sine, cos) = z.real.sin_cos();
    let t = (-2.0 * z.imaginary.abs()).exp();
    let (r, i) = if cosine {
        (cos * (1.0 + t), -z.imaginary.signum() * sine * (1.0 - t))
    } else {
        (sine * (1.0 + t), z.imaginary.signum() * cos * (1.0 - t))
    };
    let denominator = r * r + i * i;
    let factor = (LN_2 - z.imaginary.abs()).exp();
    parts(z, (r / denominator) * factor, (-i / denominator) * factor)
}
fn tangent(z: ComplexNumber, cotangent: bool) -> Result<ComplexNumber, CellError> {
    if z.imaginary.abs() <= 1.0 {
        let sine = sin_cos(z, false)?;
        let cosine = sin_cos(z, true)?;
        return if cotangent {
            divide(cosine, sine)
        } else {
            divide(sine, cosine)
        };
    }
    let (sine, cosine) = z.real.sin_cos();
    let t = (-2.0 * z.imaginary.abs()).exp();
    let u = -(-2.0 * z.imaginary.abs()).exp_m1();
    let axis = if cotangent { sine } else { cosine };
    let denominator = u * u + 4.0 * t * axis * axis;
    parts(
        z,
        4.0 * t * sine * cosine / denominator,
        z.imaginary.signum() * u * (1.0 + t) / denominator * if cotangent { -1.0 } else { 1.0 },
    )
}
/// Apply a principal complex operation, returning #NUM! for nonfinite results.
pub fn unary(z: ComplexNumber, operation: Unary) -> Result<ComplexNumber, CellError> {
    finite(z)?;
    match operation {
        Unary::Conjugate => parts(z, z.real, -z.imaginary),
        Unary::Sqrt => sqrt(z),
        Unary::Ln | Unary::Log10 | Unary::Log2 => {
            let scale = match operation {
                Unary::Log10 => LN_10,
                Unary::Log2 => LN_2,
                _ => 1.0,
            };
            parts(z, log_magnitude(z)? / scale, argument(z)? / scale)
        }
        Unary::Exp => {
            let (sine, cosine) = z.imaginary.sin_cos();
            parts(z, exp_times(z.real, cosine), exp_times(z.real, sine))
        }
        Unary::Sin | Unary::Cos => sin_cos(z, operation == Unary::Cos),
        Unary::Tan | Unary::Cot => tangent(z, operation == Unary::Cot),
        Unary::Sec | Unary::Csc => reciprocal_trig(z, operation == Unary::Sec),
        Unary::Cosh | Unary::Sinh => {
            let (sine, cosine) = z.imaginary.sin_cos();
            parts(
                z,
                hyperbolic_times(z.real, cosine, operation == Unary::Sinh),
                hyperbolic_times(z.real, sine, operation == Unary::Cosh),
            )
        }
        Unary::Sech | Unary::Csch => {
            let rotated = ComplexNumber {
                real: -z.imaginary,
                imaginary: z.real,
                suffix: z.suffix,
            };
            let value = reciprocal_trig(rotated, operation == Unary::Sech)?;
            if operation == Unary::Sech {
                Ok(value)
            } else {
                parts(z, -value.imaginary, value.real)
            }
        }
    }
}
// Exact axes avoid introducing a spurious imaginary coefficient for integer and
// half-integer powers of an axis input. Other angles retain their computed value.
fn sin_cos_pi(turns: f64) -> (f64, f64) {
    match turns % 2.0 {
        0.0 => (0.0, 1.0),
        1.0 | -1.0 => (0.0, -1.0),
        0.5 | -1.5 => (1.0, 0.0),
        -0.5 | 1.5 => (-1.0, 0.0),
        other => (PI * other).sin_cos(),
    }
}
/// Raise a complex number to a finite real power on the principal branch.
pub fn power(z: ComplexNumber, exponent: f64) -> Result<ComplexNumber, CellError> {
    finite(z)?;
    if !exponent.is_finite() {
        return Err(CellError::Num);
    }
    if z.real == 0.0 && z.imaginary == 0.0 {
        return if exponent > 0.0 {
            parts(z, 0.0, 0.0)
        } else {
            Err(CellError::Num)
        };
    }
    if exponent == 0.0 {
        return parts(z, 1.0, 0.0);
    }
    if z.imaginary == 0.0 && z.real > 0.0 {
        return parts(z, z.real.powf(exponent), 0.0);
    }
    let (sine, cosine) = if z.imaginary == 0.0 {
        sin_cos_pi(exponent)
    } else if z.real == 0.0 {
        sin_cos_pi(exponent * if z.imaginary > 0.0 { 0.5 } else { -0.5 })
    } else {
        let angle = argument(z)? * exponent;
        if !angle.is_finite() {
            return Err(CellError::Num);
        }
        angle.sin_cos()
    };
    let log = log_magnitude(z)? * exponent;
    parts(z, exp_times(log, cosine), exp_times(log, sine))
}

#[cfg(test)]
mod tests {
    use super::*;
    use poirot_coercionv2::complex::ComplexSuffix;
    fn z(real: f64, imaginary: f64) -> ComplexNumber {
        ComplexNumber {
            real,
            imaginary,
            suffix: ComplexSuffix::J,
        }
    }
    fn close(actual: ComplexNumber, real: f64, imaginary: f64) {
        for (a, b) in [(actual.real, real), (actual.imaginary, imaginary)] {
            assert!(
                (a - b).abs() <= b.abs().max(1e-300) * 4e-14,
                "{actual:?}, expected {real}+{imaginary}j"
            );
        }
        assert_eq!(actual.suffix, ComplexSuffix::J);
    }
    #[test]
    fn cartesian_arithmetic_and_independent_exact_identities() {
        assert_eq!(magnitude(z(3., 4.)), Ok(5.));
        close(add(z(1., 2.), z(3., -5.)).unwrap(), 4., -3.);
        close(subtract(z(13., 4.), z(5., 3.)).unwrap(), 8., 1.);
        close(multiply(z(1., 1.), z(1., -1.)).unwrap(), 2., 0.);
        close(divide(z(3., 4.), z(1., -2.)).unwrap(), -1., 2.);
        close(unary(z(3., 4.), Unary::Sqrt).unwrap(), 2., 1.);
        close(unary(z(2., -3.), Unary::Conjugate).unwrap(), 2., 3.);
        close(power(z(0., 1.), 2.).unwrap(), -1., 0.);
        close(power(z(4., 0.), 0.5).unwrap(), 2., 0.);
        close(power(z(-4., -0.), 0.5).unwrap(), 0., 2.);
        close(power(z(1., 1.), -1.).unwrap(), 0.5, -0.5);
    }
    #[test]
    fn scaled_arithmetic_preserves_extreme_finite_components() {
        assert!((magnitude(z(3e200, 4e200)).unwrap() / 5e200 - 1.).abs() < 2e-15);
        close(divide(z(1e308, 1e308), z(1e308, 1e308)).unwrap(), 1., 0.);
        close(
            divide(z(1e-308, 1e-308), z(1e-308, 1e-308)).unwrap(),
            1.,
            0.,
        );
        assert_eq!(divide(z(1e308, 1e-100), z(1e-100, 0.)), Err(CellError::Num));
        close(
            divide(z(1e308, 1e-100), z(1e100, 0.)).unwrap(),
            1e208,
            1e-200,
        );
        close(
            divide(z(1e308, 0.), z(1e100, 1e-300)).unwrap(),
            1e208,
            -1e-192,
        );
        close(
            unary(z(1e308, 1e308), Unary::Sqrt).unwrap(),
            1.09868411346781e154,
            4.550898605622273e153,
        );
        close(
            unary(z(1e-308, 1e-308), Unary::Sqrt).unwrap(),
            1.09868411346781e-154,
            4.550898605622273e-155,
        );
        close(
            unary(z(1e308, 1e308), Unary::Ln).unwrap(),
            709.542782232446,
            std::f64::consts::FRAC_PI_4,
        );
    }

    #[test]
    fn transcendental_components_match_independent_documented_values() {
        // Independently stated Microsoft function examples, not another engine.
        for (operation, real, imaginary) in [
            (Unary::Cosh, -27.0349456030742, 3.85115333481178),
            (Unary::Cot, 0.00490118239430447, -0.999266927805902),
            (Unary::Csc, -0.0754898329158637, 0.0648774713706355),
            (Unary::Csch, -0.036275889628626, -0.0051744731840194),
            (Unary::Sec, -0.0652940278579471, -0.0752249603027732),
            (Unary::Sech, -0.0362534969158689, -0.00516434460775318),
            (Unary::Sin, -7.61923172032141, -6.548120040911),
            (Unary::Sinh, -27.0168132580039, 3.85373803791938),
            (Unary::Tan, 0.00490825806749606, 1.00070953606723),
        ] {
            close(unary(z(4., 3.), operation).unwrap(), real, imaginary);
        }
        close(
            unary(z(1., 1.), Unary::Cos).unwrap(),
            0.833730025131149,
            -0.988897705762865,
        );
        close(
            unary(z(1., 1.), Unary::Exp).unwrap(),
            1.46869393991589,
            2.28735528717884,
        );
        close(
            unary(z(3., 4.), Unary::Ln).unwrap(),
            1.6094379124341,
            0.927295218001612,
        );
        close(
            unary(z(3., 4.), Unary::Log10).unwrap(),
            0.698970004336019,
            0.402719196273373,
        );
        close(
            unary(z(3., 4.), Unary::Log2).unwrap(),
            2.32192809488736,
            1.33780421245098,
        );
    }
    #[test]
    fn stable_ratios_zero_domains_and_principal_branch() {
        close(
            unary(z(f64::from_bits(1), 0.), Unary::Sin).unwrap(),
            f64::from_bits(1),
            0.,
        );
        close(unary(z(2., 1000.), Unary::Tan).unwrap(), 0., 1.);
        close(unary(z(2., -1000.), Unary::Cot).unwrap(), 0., 1.);
        close(unary(z(2., 1000.), Unary::Sec).unwrap(), 0., 0.);
        close(unary(z(1000., 2.), Unary::Csch).unwrap(), 0., 0.);
        close(unary(z(1e-300, 0.), Unary::Csc).unwrap(), 1e300, 0.);
        close(unary(z(1e-300, 0.), Unary::Cot).unwrap(), 1e300, 0.);
        assert_eq!(argument(z(-1., -0.)), Ok(PI));
        assert!(argument(z(-1., -1e-10)).unwrap() < 0.);
        assert!(unary(z(-1., -1e-10), Unary::Sqrt).unwrap().imaginary < 0.);
        assert!(unary(z(-1., 1e-10), Unary::Sqrt).unwrap().imaginary > 0.);
        for operation in [
            Unary::Ln,
            Unary::Log10,
            Unary::Log2,
            Unary::Cot,
            Unary::Csc,
            Unary::Csch,
        ] {
            assert_eq!(unary(z(0., 0.), operation), Err(CellError::Num));
        }
        assert_eq!(argument(z(0., 0.)), Err(CellError::Div0));
        assert_eq!(divide(z(1., 0.), z(0., 0.)), Err(CellError::Num));
        assert_eq!(power(z(0., 0.), 0.), Err(CellError::Num));
        assert_eq!(power(z(0., 0.), -1.), Err(CellError::Num));
        close(power(z(0., 0.), 1.).unwrap(), 0., 0.);
        assert_eq!(unary(z(1000., 1.), Unary::Exp), Err(CellError::Num));
    }
}
