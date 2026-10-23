//! Lazy engineering execution through engine-owned coercion and reference reads.
/// Operand provenance used by coercion.
pub use poirot_functions_core::Origin;
/// Scalar paired with its source provenance.
pub use poirot_functions_core::ResolvedScalar;
/// Scalar representation accepted by engineering helpers.
pub use poirot_functions_core::Scalar;
use poirot_functions_engineering as domain;
use poirot_model::CellError;
/// Radix, complex number, and special engineering operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineeringFunction {
    /// Bessel function of the first kind.
    Besselj,
    /// Binary text to decimal.
    Bin2dec,
    /// Construct complex number text.
    Complex,
    /// Decimal to binary text.
    Dec2bin,
    /// Decimal to hexadecimal text.
    Dec2hex,
    /// Test numeric equality.
    Delta,
    /// Hexadecimal text to decimal.
    Hex2dec,
    /// Extract an imaginary coefficient.
    Imaginary,
    /// Extract a real coefficient.
    Imreal,
    /// Octal text to decimal.
    Oct2dec,
}
/// Missing host coercions required by selected engineering operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedCapability {
    /// The real Bessel kernel exceeded its bounded order or iteration envelope.
    BesselComputationalLimit,
    /// Omitted-argument text coercion is unavailable.
    MissingStringCoercion,
    /// Omitted-argument numeric coercion is unavailable.
    MissingNumericCoercion,
}
/// Engineering failures preserving semantic and operational error domains.
pub type Failure<O> = crate::adapters::failure::AdapterFailure<O, UnsupportedCapability>;
/// Engineering result before conversion to an evaluator value.
#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    /// Numeric result.
    Number(f64),
    /// Radix or complex number text.
    Text(String),
}
/// Explicit adapter contracts; the standalone helpers retain pinned defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticsPolicy {
    /// Pinned Apache POI behavior.
    PinnedPoi,
    /// Documented Excel behavior.
    ExcelDocumentation,
}
/// Resolve only the source-selected singleton, never eagerly evaluate unused
/// arguments/area cells. Operational failures must survive engine IFERROR.
pub trait EvaluationPort {
    /// Deferred engine-owned operand handle.
    type Argument;
    /// Host operational failure type.
    type Operational;
    /// Resolve only the source-selected scalar and preserve its origin.
    fn resolve_scalar(
        &mut self,
        arg: &Self::Argument,
    ) -> Result<ResolvedScalar, Failure<Self::Operational>>;
    /// Admit work before performing it.
    fn charge_work(&mut self, units: u64) -> Result<(), Self::Operational>;
    /// Check text capacity before copying or allocating.
    fn check_text(&mut self, bytes: usize) -> Result<(), Self::Operational>;
    /// Exact POI NumberToTextConverter spelling, shared poirot-numeric formatter.
    fn coercev2_number_to_text(&mut self, n: f64) -> Result<String, Failure<Self::Operational>>;
    /// OperandResolver.parseDouble only: no date fallback.
    fn coercev2_decimal_text(
        &mut self,
        text: &str,
    ) -> Result<Option<f64>, Failure<Self::Operational>>;
    /// Ordinary OperandResolver.coerceValueToDouble (includes date fallback).
    fn coercev2_number_text(&mut self, text: &str) -> Result<f64, Failure<Self::Operational>>;
    /// Apply explicit omitted-argument text coercion or reject the capability.
    fn coercev2_missing_text(&mut self) -> Result<String, Failure<Self::Operational>> {
        Err(Failure::UnsupportedCapability(
            UnsupportedCapability::MissingStringCoercion,
        ))
    }
    /// Apply explicit omitted-argument numeric coercion or reject the capability.
    fn coercev2_missing_number(&mut self) -> Result<f64, Failure<Self::Operational>> {
        Err(Failure::UnsupportedCapability(
            UnsupportedCapability::MissingNumericCoercion,
        ))
    }
}
fn work<P: EvaluationPort>(p: &mut P, n: u64) -> Result<(), Failure<P::Operational>> {
    p.charge_work(n).map_err(Failure::Operational)
}
fn checked_text<P: EvaluationPort>(
    p: &mut P,
    s: String,
) -> Result<String, Failure<P::Operational>> {
    p.check_text(s.len()).map_err(Failure::Operational)?;
    work(p, s.len() as u64)?;
    Ok(s)
}
fn scalar<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<Scalar, Failure<P::Operational>> {
    work(p, 1)?;
    match p.resolve_scalar(a)?.value {
        Scalar::Error(e) => Err(Failure::Spreadsheet(e)),
        v => Ok(v),
    }
}
fn coercev2_engineering_argument_text<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<String, Failure<P::Operational>> {
    let v = scalar(p, a)?;
    coercev2_engineering_scalar_text(p, v)
}
fn coercev2_engineering_scalar_text<P: EvaluationPort>(
    p: &mut P,
    v: Scalar,
) -> Result<String, Failure<P::Operational>> {
    use poirot_coercionv2::compatibility::{OwnedDeferredText, coercev2_owned_text};
    p.check_text(64).map_err(Failure::Operational)?;
    let text = coercev2_owned_text(v, |request| match request {
        OwnedDeferredText::Number(n) => p.coercev2_number_to_text(n),
        OwnedDeferredText::Text(t) => Ok(t),
        OwnedDeferredText::Missing => p.coercev2_missing_text(),
    })?;
    checked_text(p, text)
}
fn coercev2_engineering_decimal_number<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<f64, Failure<P::Operational>> {
    let s = coercev2_engineering_argument_text(p, a)?;
    p.coercev2_decimal_text(&s)?
        .ok_or(Failure::Spreadsheet(CellError::Value))
}
fn coercev2_engineering_number<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<f64, Failure<P::Operational>> {
    use poirot_coercionv2::compatibility::{DeferredNumber, NumberPolicy, coercev2_number};
    let value = scalar(p, a)?;
    coercev2_number(
        (&value).into(),
        NumberPolicy::Preserve,
        |input| match input {
            DeferredNumber::Missing => p.coercev2_missing_number(),
            DeferredNumber::Text(poirot_functions_core::TextRef::Utf8(text)) => {
                p.check_text(text.len()).map_err(Failure::Operational)?;
                work(p, text.len() as u64)?;
                p.coercev2_number_text(text)
            }
            DeferredNumber::Text(_) => Err(CellError::Value.into()),
        },
    )
}
fn domain_error<O>(e: domain::EngineeringError) -> Failure<O> {
    Failure::Spreadsheet(match e {
        domain::EngineeringError::Num => CellError::Num,
        domain::EngineeringError::Value => CellError::Value,
    })
}
impl EngineeringFunction {
    /// Check pinned argument-count rules before evaluating operands.
    pub const fn accepts(self, n: usize) -> bool {
        match self {
            Self::Complex => n == 2 || n == 3,
            Self::Dec2bin | Self::Dec2hex => n == 1 || n == 2,
            Self::Besselj | Self::Delta => n == 2,
            _ => n == 1,
        }
    }
    /// Check argument-count rules under the selected compatibility policy.
    pub const fn accepts_with_policy(self, n: usize, policy: SemanticsPolicy) -> bool {
        if matches!(self, Self::Delta) && matches!(policy, SemanticsPolicy::ExcelDocumentation) {
            n == 1 || n == 2
        } else {
            self.accepts(n)
        }
    }
}
/// Execute an engineering operation under pinned compatibility rules.
pub fn evaluate<P: EvaluationPort>(
    p: &mut P,
    f: EngineeringFunction,
    args: &[P::Argument],
) -> Result<Output, Failure<P::Operational>> {
    evaluate_with_policy(p, f, args, SemanticsPolicy::PinnedPoi)
}
/// Execute an engineering operation under explicit compatibility rules.
pub fn evaluate_with_policy<P: EvaluationPort>(
    p: &mut P,
    f: EngineeringFunction,
    args: &[P::Argument],
    policy: SemanticsPolicy,
) -> Result<Output, Failure<P::Operational>> {
    use EngineeringFunction::*;
    let excel = policy == SemanticsPolicy::ExcelDocumentation;
    if !f.accepts_with_policy(args.len(), policy) {
        return Err(Failure::Spreadsheet(CellError::Value));
    }
    work(p, 1)?;
    let out = match f {
        Imaginary | Imreal if excel => {
            let value = scalar(p, args.first().ok_or(CellError::Value)?)?;
            if let Scalar::Text(ref text) = value {
                p.check_text(text.len()).map_err(Failure::Operational)?;
                work(p, text.len() as u64 + 1)?;
            }
            let value = match poirot_coercionv2::complex::coercev2_complex_scalar(value)? {
                Some(value) => value,
                None => {
                    let text = p.coercev2_missing_text()?;
                    p.check_text(text.len()).map_err(Failure::Operational)?;
                    work(p, text.len() as u64 + 1)?;
                    poirot_coercionv2::complex::coercev2_complex_text(&text)?
                }
            };
            Output::Number(if f == Imaginary {
                domain::complex_imaginary(value)
            } else {
                domain::complex_real(value)
            })
        }
        Bin2dec | Hex2dec | Oct2dec | Imaginary | Imreal => {
            let s = coercev2_engineering_argument_text(p, args.first().ok_or(CellError::Value)?)?;
            // Regex/output allocation bounded by the input plus a sign/digit.
            p.check_text(s.len().saturating_add(2))
                .map_err(Failure::Operational)?;
            work(p, s.len() as u64 + 1)?;
            match f {
                Bin2dec => {
                    if excel && s.bytes().any(|digit| digit != b'0' && digit != b'1') {
                        return Err(Failure::Spreadsheet(CellError::Num));
                    }
                    Output::Number(domain::bin2dec(&s).map_err(domain_error)?)
                }
                Hex2dec => Output::Number(domain::hex2dec(&s).map_err(domain_error)?),
                Oct2dec => Output::Number(domain::oct2dec(&s).map_err(domain_error)?),
                Imaginary | Imreal => {
                    let coefficient = if f == Imaginary {
                        domain::imaginary(&s)
                    } else {
                        domain::imreal(&s)
                    }
                    .map_err(domain_error)?;
                    Output::Text(coefficient)
                }
                _ => return Err(CellError::Value.into()),
            }
        }
        Dec2bin | Dec2hex => {
            let n = coercev2_engineering_decimal_number(p, args.first().ok_or(CellError::Value)?)?;
            // Validate range BEFORE resolving places, including out-of-domain n.
            let half = if f == Dec2bin { 512.0 } else { 549755813888.0 };
            if !n.is_finite() || n <= -half - 1.0 || n >= half {
                return Err(Failure::Spreadsheet(CellError::Num));
            }
            let places = if n.trunc() < 0.0 || args.len() == 1 {
                None
            } else {
                Some(coercev2_engineering_decimal_number(
                    p,
                    args.get(1).ok_or(CellError::Value)?,
                )?)
            };
            p.check_text(10).map_err(Failure::Operational)?;
            work(p, 10)?;
            Output::Text(
                if f == Dec2bin {
                    domain::dec2bin(n, places)
                } else {
                    domain::dec2hex(n, places)
                }
                .map_err(domain_error)?,
            )
        }
        Delta | Besselj => {
            let x = coercev2_engineering_decimal_number(p, args.first().ok_or(CellError::Value)?)?;
            let y = if args.len() == 1 {
                0.0
            } else if excel && f == Delta {
                let value = scalar(p, args.get(1).ok_or(CellError::Value)?)?;
                if value == Scalar::Missing {
                    0.0
                } else {
                    let s = coercev2_engineering_scalar_text(p, value)?;
                    p.coercev2_decimal_text(&s)?
                        .ok_or(Failure::Spreadsheet(CellError::Value))?
                }
            } else {
                coercev2_engineering_decimal_number(p, args.get(1).ok_or(CellError::Value)?)?
            };
            if f == Delta {
                Output::Number(domain::delta(x, y))
            } else if excel {
                use domain::special::{self, Bessel};
                let value = special::bessel(Bessel::First, x, y, |units| p.charge_work(units))
                    .map_err(|failure| match failure {
                        special::Failure::Number => Failure::Spreadsheet(CellError::Num),
                        special::Failure::Operational(error) => Failure::Operational(error),
                        special::Failure::ComputationalLimit => Failure::UnsupportedCapability(
                            UnsupportedCapability::BesselComputationalLimit,
                        ),
                    })?;
                Output::Number(value)
            } else {
                // libm recurrence may scale with integer order; charge before work.
                let units = if y.is_finite() && y >= 0.0 {
                    (y.trunc() as u64).saturating_add(10001)
                } else {
                    1
                };
                work(p, units)?;
                Output::Number(domain::besselj(x, y).map_err(domain_error)?)
            }
        }
        Complex => {
            let r = coercev2_engineering_number(p, args.first().ok_or(CellError::Value)?)?;
            let i = coercev2_engineering_number(p, args.get(1).ok_or(CellError::Value)?)?;
            let suffix = if args.len() == 3 {
                coercev2_engineering_argument_text(p, args.get(2).ok_or(CellError::Value)?)?
            } else {
                "i".into()
            };
            let unit = poirot_coercionv2::complex::coercev2_complex_suffix(&suffix)?;
            p.check_text(132).map_err(Failure::Operational)?;
            work(p, 132)?;
            // Format only components actually emitted by the pinned source.
            let rt = if r != 0.0 {
                // TODO(coercion-non-argument): [LOW] Rendering a computed complex component at spreadsheet precision can discard digits in the returned text.
                p.coercev2_number_to_text(r)?
            } else {
                String::new()
            };
            let it = if i != 0.0 && i != 1.0 && i != -1.0 {
                // TODO(coercion-non-argument): [LOW] Rendering a computed complex component at spreadsheet precision can discard digits in the returned text.
                p.coercev2_number_to_text(i)?
            } else {
                String::new()
            };
            let format_number = |n: f64| {
                if n.to_bits() == r.to_bits() {
                    rt.clone()
                } else {
                    it.clone()
                }
            };
            // TODO(coercion-non-argument): [LOW] Result spelling uses the selected spreadsheet precision and compatibility policy.
            Output::Text(if excel {
                poirot_coercionv2::complex::coerce_complex_result(
                    poirot_coercionv2::complex::ComplexNumber {
                        real: r,
                        imaginary: i,
                        suffix: unit,
                    },
                    format_number,
                )?
            } else {
                domain::complex(r, i, &suffix, format_number).map_err(domain_error)?
            })
        }
    };
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    /// Test engineering host recording operand reads and enforcing helper work limits.
    struct Port {
        reads: Vec<usize>,
        remaining: u64,
    }
    impl EvaluationPort for Port {
        /// Deferred argument identity supplied to the family's host port.
        type Argument = Scalar;
        /// Host abort channel kept outside catchable spreadsheet values.
        type Operational = &'static str;
        fn resolve_scalar(
            &mut self,
            a: &Scalar,
        ) -> Result<ResolvedScalar, Failure<Self::Operational>> {
            self.reads.push(self.reads.len());
            Ok(ResolvedScalar {
                value: a.clone(),
                origin: Origin::Direct,
            })
        }
        fn charge_work(&mut self, n: u64) -> Result<(), Self::Operational> {
            if n > self.remaining {
                Err("budget")
            } else {
                self.remaining -= n;
                Ok(())
            }
        }
        fn check_text(&mut self, _: usize) -> Result<(), Self::Operational> {
            Ok(())
        }
        fn coercev2_number_to_text(
            &mut self,
            n: f64,
        ) -> Result<String, Failure<Self::Operational>> {
            Ok(n.to_string())
        }
        fn coercev2_decimal_text(
            &mut self,
            s: &str,
        ) -> Result<Option<f64>, Failure<Self::Operational>> {
            Ok(s.parse().ok())
        }
        fn coercev2_number_text(&mut self, s: &str) -> Result<f64, Failure<Self::Operational>> {
            s.parse()
                .map_err(|_| Failure::Spreadsheet(CellError::Value))
        }
    }
    fn port() -> Port {
        Port {
            reads: vec![],
            remaining: 100000,
        }
    }
    #[test]
    fn pinned_negative_conversion_does_not_read_places() {
        for f in [EngineeringFunction::Dec2bin, EngineeringFunction::Dec2hex] {
            let mut p = port();
            let result = evaluate(
                &mut p,
                f,
                &[Scalar::Number(-1.0), Scalar::Error(CellError::NA)],
            );
            assert!(matches!(result, Ok(Output::Text(_))));
            assert_eq!(p.reads.len(), 1);
        }
    }
    #[test]
    fn pinned_range_and_arity_precede_places() {
        let mut p = port();
        assert_eq!(
            evaluate(
                &mut p,
                EngineeringFunction::Dec2bin,
                &[Scalar::Number(512.0), Scalar::Error(CellError::NA)]
            ),
            Err(Failure::Spreadsheet(CellError::Num))
        );
        assert_eq!(p.reads.len(), 1);
        let mut p = port();
        assert_eq!(
            evaluate(
                &mut p,
                EngineeringFunction::Delta,
                &[Scalar::Error(CellError::NA)]
            ),
            Err(Failure::Spreadsheet(CellError::Value))
        );
        assert!(p.reads.is_empty());
    }
    #[test]
    fn parse_coercion_and_operational_failure_stay_distinct() {
        let mut p = port();
        assert_eq!(
            evaluate(
                &mut p,
                EngineeringFunction::Delta,
                &[Scalar::Blank, Scalar::Error(CellError::NA)]
            ),
            Err(Failure::Spreadsheet(CellError::Value))
        );
        assert_eq!(p.reads.len(), 1);
        let mut p = port();
        p.remaining = 0;
        assert_eq!(
            evaluate(
                &mut p,
                EngineeringFunction::Bin2dec,
                &[Scalar::Text("10".into())]
            ),
            Err(Failure::Operational("budget"))
        );
        assert!(p.reads.is_empty());
    }
    #[test]
    fn pinned_complex_and_complex_parts_are_text() {
        let mut p = port();
        assert_eq!(
            evaluate(
                &mut p,
                EngineeringFunction::Complex,
                &[Scalar::Number(3.0), Scalar::Number(-1.0)]
            ),
            Ok(Output::Text("3i".into()))
        );
        assert_eq!(
            evaluate(
                &mut p,
                EngineeringFunction::Imaginary,
                &[Scalar::Text("2i".into())]
            ),
            Ok(Output::Text("1".into()))
        );
    }
    #[test]
    fn documented_policy_is_explicit_and_uses_shared_coefficient_parsing() {
        let policy = SemanticsPolicy::ExcelDocumentation;
        for (f, input, expected) in [
            (EngineeringFunction::Imreal, "-2+3i", -2.0),
            (EngineeringFunction::Imaginary, "-2+3i", 3.0),
        ] {
            assert_eq!(
                evaluate_with_policy(&mut port(), f, &[Scalar::Text(input.into())], policy),
                Ok(Output::Number(expected))
            );
        }
        for input in ["2", "１２", "x+00000001", "1000000002"] {
            assert_eq!(
                evaluate_with_policy(
                    &mut port(),
                    EngineeringFunction::Bin2dec,
                    &[Scalar::Text(input.into())],
                    policy
                ),
                Err(Failure::Spreadsheet(CellError::Num))
            );
        }
        assert!(EngineeringFunction::Delta.accepts_with_policy(1, policy));
        assert!(!EngineeringFunction::Delta.accepts(1));
        assert_eq!(
            evaluate_with_policy(
                &mut port(),
                EngineeringFunction::Delta,
                &[Scalar::Number(0.0), Scalar::Missing],
                policy
            ),
            Ok(Output::Number(1.0))
        );
        assert_eq!(
            evaluate_with_policy(
                &mut port(),
                EngineeringFunction::Delta,
                &[Scalar::Number(0.0)],
                policy
            ),
            Ok(Output::Number(1.0))
        );
        let mut p = port();
        assert_eq!(
            evaluate_with_policy(
                &mut p,
                EngineeringFunction::Delta,
                &[
                    Scalar::Error(CellError::NA),
                    Scalar::Number(0.0),
                    Scalar::Number(0.0)
                ],
                policy
            ),
            Err(Failure::Spreadsheet(CellError::Value))
        );
        assert!(p.reads.is_empty());
        let mut p = port();
        p.remaining = 0;
        assert_eq!(
            evaluate_with_policy(
                &mut p,
                EngineeringFunction::Imreal,
                &[Scalar::Text("-2+3i".into())],
                policy
            ),
            Err(Failure::Operational("budget"))
        );
        assert!(p.reads.is_empty());
    }
}
