//! Lazy math execution through engine-owned coercion, references and random streams.
/// Scalar provenance used by coercion rules.
pub use poirot_functions_core::Origin;
/// Scalar value paired with its provenance.
pub use poirot_functions_core::ResolvedScalar;
/// Helper-domain scalar representation.
pub use poirot_functions_core::Scalar;
use poirot_functions_math as domain;
use poirot_model::CellError;

/// Compatibility rules selected for mathematical execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticsPolicy {
    /// Pinned Apache POI behavior.
    PinnedPoi,
    /// Documented Excel behavior.
    ExcelDocumentation,
}

/// Mathematical operations implemented through this lazy adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MathFunction {
    /// Inverse hyperbolic cosine.
    Acosh,
    /// Inverse hyperbolic sine.
    Asinh,
    /// Two-coordinate inverse tangent.
    Atan2,
    /// Inverse hyperbolic tangent.
    Atanh,
    /// CEILING.MATH rounding.
    CeilingMath,
    /// CEILING.PRECISE rounding.
    CeilingPrecise,
    /// Number of combinations.
    Combin,
    /// Combinations with replacement under the documented domain.
    Combina,
    /// Hyperbolic cosine.
    Cosh,
    /// Round away from zero to an even integer.
    Even,
    /// Factorial.
    Fact,
    /// Double factorial.
    Factdouble,
    /// FLOOR.MATH rounding.
    FloorMath,
    /// FLOOR.PRECISE rounding.
    FloorPrecise,
    /// Greatest common divisor.
    Gcd,
    /// Least common multiple.
    Lcm,
    /// Round to a multiple.
    Mround,
    /// Round away from zero to an odd integer.
    Odd,
    /// Integer quotient.
    Quotient,
    /// Uniform random value in `[0, 1)`.
    Rand,
    /// Inclusive random integer interval.
    Randbetween,
    /// Hyperbolic sine.
    Sinh,
    /// Square root of the argument multiplied by pi.
    Sqrtpi,
    /// Hyperbolic tangent.
    Tanh,
}
/// Host capabilities required by selected mathematical operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsupportedCapability {
    /// No valid calculation-owned uniform stream is available.
    RandomStream,
    /// Inclusive integer sampling is unavailable for the requested bounds.
    RandomIntegerInterval,
    /// Omitted-argument numeric coercion is unavailable.
    MissingNumericArgument,
}
/// Mathematical adapter failures with operational errors preserved.
pub type Failure<O> = crate::adapters::failure::AdapterFailure<O, UnsupportedCapability>;

/// The engine owns references and validated rectangular arrays. Resolve exactly
/// one cell using OperandResolver selection: first sheet for a RefEval; caller
/// row for a one-column area; caller column for a one-row area; caller coordinates
/// for a multi-row multi-column area. Out-of-range selection yields Value.
/// Array-formula output coordinates use getElementFromArray (out-of-range NA).
/// This method must not eagerly evaluate the rest of an area. Operational faults
/// must remain Failure::Operational, including faults during reference traversal.
pub trait EvaluationPort {
    /// Engine-owned deferred operand handle.
    type Argument;
    /// Host's operational failure type.
    type Operational;
    /// Resolve only the caller-selected scalar and preserve its origin.
    fn resolve_scalar(
        &mut self,
        arg: &Self::Argument,
    ) -> Result<ResolvedScalar, Failure<Self::Operational>>;
    /// Admit work before performing helper operations.
    fn charge_work(&mut self, units: u64) -> Result<(), Self::Operational>;
    /// Check collection capacity before allocation.
    fn check_collection(&mut self, elements: usize) -> Result<(), Self::Operational>;
    /// Check text capacity before copying or allocating.
    fn check_text(&mut self, bytes: usize) -> Result<(), Self::Operational>;
    /// Existing engine numeric coercion: numeric grammar incl Java hexadecimal
    /// parsing, then POI date/time fallback. Called only after text budget check.
    fn coercev2_text_number(&mut self, text: &str) -> Result<f64, Failure<Self::Operational>>;
    /// MissingArgEval is not BlankEval in POI's OperandResolver. The engine owns
    /// independently observed missing-argument compatibility (do not assume zero).
    fn coercev2_missing_number(&mut self) -> Result<f64, Failure<Self::Operational>> {
        Err(Failure::UnsupportedCapability(
            UnsupportedCapability::MissingNumericArgument,
        ))
    }
    /// Consume one calculation-owned uniform draw in [0, 1). Ports without a
    /// random stream must refuse rather than using ambient state or a fallback.
    fn random_unit(&mut self) -> Result<f64, Failure<Self::Operational>> {
        Err(Failure::UnsupportedCapability(
            UnsupportedCapability::RandomStream,
        ))
    }
    /// Consume a calculation-owned unbiased inclusive integer draw. Bounds remain
    /// f64 to avoid truncating large representable intervals. The engine must
    /// reject intervals outside its proven entropy/representation capability.
    fn random_integer(
        &mut self,
        _bottom: f64,
        _top: f64,
    ) -> Result<f64, Failure<Self::Operational>> {
        Err(Failure::UnsupportedCapability(
            UnsupportedCapability::RandomIntegerInterval,
        ))
    }
}
fn work<P: EvaluationPort>(p: &mut P, n: u64) -> Result<(), Failure<P::Operational>> {
    p.charge_work(n).map_err(Failure::Operational)
}
fn coercev2_math_number<P: EvaluationPort>(
    p: &mut P,
    arg: &P::Argument,
) -> Result<f64, Failure<P::Operational>> {
    use poirot_coercionv2::compatibility::{DeferredNumber, NumberPolicy, coercev2_number};
    work(p, 1)?;
    let scalar = p.resolve_scalar(arg)?;
    coercev2_number(
        (&scalar.value).into(),
        NumberPolicy::Finite,
        |input| match input {
            DeferredNumber::Missing => p.coercev2_missing_number(),
            DeferredNumber::Text(poirot_functions_core::TextRef::Utf8(text)) => {
                p.check_text(text.len()).map_err(Failure::Operational)?;
                work(p, text.len() as u64)?;
                p.coercev2_text_number(text)
            }
            DeferredNumber::Text(_) => Err(CellError::Value.into()),
        },
    )
}
fn collapse_operand_error<O>(f: Failure<O>) -> Failure<O> {
    match f {
        Failure::Spreadsheet(_) => Failure::Spreadsheet(CellError::Value),
        other => other,
    }
}
impl MathFunction {
    /// Whether this operation consumes a calculation-owned random stream.
    pub const fn volatile(self) -> bool {
        matches!(self, Self::Rand | Self::Randbetween)
    }
    /// Check pinned argument-count rules without evaluating operands.
    pub const fn accepts(self, count: usize) -> bool {
        match self {
            Self::Rand => count == 0,
            Self::CeilingMath | Self::FloorMath | Self::CeilingPrecise | Self::FloorPrecise => {
                count >= 1
            }
            Self::Gcd | Self::Lcm => count >= 1,
            Self::Atan2
            | Self::Combina
            | Self::Combin
            | Self::Mround
            | Self::Quotient
            | Self::Randbetween => count == 2,
            _ => count == 1,
        }
    }
}
/// POI FreeRef rounding functions ignore extra args; only their first 2/3 are
/// coerced. Evaluator must call this before evaluating any argument so wrong
/// arity never consumes references/RNG and ignored extras remain lazy.
pub fn evaluate<P: EvaluationPort>(
    p: &mut P,
    function: MathFunction,
    args: &[P::Argument],
) -> Result<f64, Failure<P::Operational>> {
    evaluate_with_policy(p, function, args, SemanticsPolicy::PinnedPoi)
}
/// Execute lazy operands under explicitly selected compatibility rules.
pub fn evaluate_with_policy<P: EvaluationPort>(
    p: &mut P,
    function: MathFunction,
    args: &[P::Argument],
    policy: SemanticsPolicy,
) -> Result<f64, Failure<P::Operational>> {
    use MathFunction::*;
    if !function.accepts(args.len()) {
        return Err(CellError::Value.into());
    }
    work(p, 1)?;
    if function == Rand {
        let r = p.random_unit()?;
        return if r.is_finite() && (0.0..1.0).contains(&r) {
            Ok(r)
        } else {
            Err(Failure::UnsupportedCapability(
                UnsupportedCapability::RandomStream,
            ))
        };
    }
    let collapse = matches!(function, Gcd | Lcm | Quotient | Randbetween);
    let read = |p: &mut P, index: usize| {
        let r = coercev2_math_number(p, args.get(index).ok_or(CellError::Value)?);
        if collapse {
            r.map_err(collapse_operand_error)
        } else {
            r
        }
    };
    if function == Combina {
        let mut number = |index: usize| {
            work(p, 1)?;
            let scalar = p.resolve_scalar(args.get(index).ok_or(CellError::Value)?)?;
            poirot_coercionv2::combinations::coercev2_combina_number(
                (&scalar.value).into(),
                |text| match text {
                    poirot_functions_core::TextRef::Utf8(text) => {
                        p.check_text(text.len()).map_err(Failure::Operational)?;
                        work(p, text.len() as u64)?;
                        p.coercev2_text_number(text)
                    }
                    _ => Err(CellError::Value.into()),
                },
            )
        };
        let n = number(0)?;
        let k = number(1)?;
        // Kernel work and integers are bounded independently of operand size.
        work(p, 1024)?;
        p.check_collection(512).map_err(Failure::Operational)?;
        return domain::combina(n, k).map_err(Failure::Spreadsheet);
    }
    let x = read(p, 0)?;
    // Variadic families validate each operand in source order before moving on.
    // GCD checks raw value (before truncation) and LCM checks raw negativity.
    if matches!(function, Gcd | Lcm) {
        p.check_collection(args.len())
            .map_err(Failure::Operational)?;
        work(p, args.len() as u64)?;
        let mut values = Vec::with_capacity(args.len());
        for index in 0..args.len() {
            let n = if index == 0 { x } else { read(p, index)? };
            if n < 0.0 || (function == Gcd && n > 9007199254740992.0) {
                return Err(CellError::Num.into());
            }
            values.push(n);
        }
        // Euclidean iterations are bounded by at most 93 for each 64-bit pair.
        work(p, (args.len() as u64).saturating_mul(94))?;
        return if function == Gcd {
            domain::gcd(&values)
        } else {
            domain::lcm(&values)
        }
        .map_err(Failure::Spreadsheet);
    }
    let result = match function {
        Acosh => domain::acosh(x),
        Asinh => domain::asinh(x),
        Atanh => domain::atanh(x),
        Cosh => domain::cosh(x),
        Sinh => domain::sinh(x),
        Tanh => domain::tanh(x),
        Sqrtpi => domain::sqrtpi(x),
        Even => domain::even(x),
        Odd => domain::odd(x),
        Fact => {
            work(p, 170)?;
            domain::fact(x)
        }
        Factdouble => {
            p.check_collection(301).map_err(Failure::Operational)?;
            work(p, 301)?;
            domain::factdouble(x)
        }
        Atan2 => domain::atan2(x, read(p, 1)?),
        Quotient => domain::quotient(x, read(p, 1)?),
        Combina => return Err(CellError::Value.into()),
        Combin => {
            let k = read(p, 1)?;
            work(p, 341)?;
            domain::combin(x, k)
        }
        Mround => {
            let m = read(p, 1)?;
            p.check_collection(2048).map_err(Failure::Operational)?;
            work(p, 2048)?;
            domain::mround(x, m)
        }
        CeilingMath | FloorMath | CeilingPrecise | FloorPrecise => {
            let s = if args.len() > 1 { read(p, 1)? } else { 1.0 };
            // Source returns immediately on zero significance, ignoring mode.
            if s == 0.0 {
                return Ok(0.0);
            }
            let mode = if matches!(function, CeilingMath | FloorMath) && args.len() > 2 {
                read(p, 2)?
            } else {
                0.0
            };
            // Both MATH functions use any nonzero mode as a direction flag in
            // Excel. Their pinned POI kernels reverse only for a negative mode.
            let mode = if policy == SemanticsPolicy::ExcelDocumentation && mode != 0.0 {
                -1.0
            } else {
                mode
            };
            p.check_collection(2048).map_err(Failure::Operational)?;
            work(p, 2048)?;
            match function {
                CeilingMath => domain::ceiling_math(x, s, mode),
                FloorMath => domain::floor_math(x, s, mode),
                CeilingPrecise => domain::ceiling_precise(x, s),
                FloorPrecise => domain::floor_precise(x, s),
                _ => return Err(CellError::Value.into()),
            }
        }
        Randbetween => {
            let top = read(p, 1)?;
            if x > top {
                return Err(CellError::Num.into());
            }
            let bottom = x.ceil();
            let top = top.floor().max(bottom);
            let r = p.random_integer(bottom, top)?;
            if r.is_finite() && r == r.trunc() && r >= bottom && r <= top {
                return Ok(r);
            }
            return Err(Failure::UnsupportedCapability(
                UnsupportedCapability::RandomIntegerInterval,
            ));
        }
        // SAFETY: Rand returns in its initial branch; Gcd and Lcm return from the preceding collection branch before this match.
        Rand | Gcd | Lcm => unreachable!(),
    };
    result.map_err(Failure::Spreadsheet)
}

#[cfg(test)]
mod adapter_tests {
    use super::*;
    /// Test scalar with controlled provenance.
    #[derive(Clone)]
    struct Arg(Scalar, Origin);
    /// Test host recording reads, work, and random requests under configurable limits.
    #[derive(Default)]
    struct Port {
        reads: usize,
        work: u64,
        work_limit: Option<u64>,
        collection_limit: Option<usize>,
        text_limit: Option<usize>,
        random: Option<f64>,
        random_calls: usize,
        interval: Option<(f64, f64)>,
    }
    impl EvaluationPort for Port {
        /// Deferred argument identity supplied to the family's host port.
        type Argument = Arg;
        /// Host abort channel kept outside catchable spreadsheet values.
        type Operational = &'static str;
        fn resolve_scalar(
            &mut self,
            a: &Arg,
        ) -> Result<ResolvedScalar, Failure<Self::Operational>> {
            self.reads += 1;
            Ok(ResolvedScalar {
                value: a.0.clone(),
                origin: a.1,
            })
        }
        fn charge_work(&mut self, n: u64) -> Result<(), Self::Operational> {
            if self
                .work_limit
                .is_some_and(|max| self.work.saturating_add(n) > max)
            {
                return Err("work");
            }
            self.work += n;
            Ok(())
        }
        fn check_collection(&mut self, n: usize) -> Result<(), Self::Operational> {
            if self.collection_limit.is_some_and(|max| n > max) {
                Err("collection")
            } else {
                Ok(())
            }
        }
        fn check_text(&mut self, n: usize) -> Result<(), Self::Operational> {
            if self.text_limit.is_some_and(|max| n > max) {
                Err("text")
            } else {
                Ok(())
            }
        }
        fn coercev2_text_number(&mut self, t: &str) -> Result<f64, Failure<Self::Operational>> {
            t.trim()
                .parse()
                .map_err(|_| Failure::Spreadsheet(CellError::Value))
        }
        fn random_unit(&mut self) -> Result<f64, Failure<Self::Operational>> {
            self.random_calls += 1;
            self.random.ok_or(Failure::UnsupportedCapability(
                UnsupportedCapability::RandomStream,
            ))
        }
        fn random_integer(&mut self, b: f64, t: f64) -> Result<f64, Failure<Self::Operational>> {
            self.random_calls += 1;
            self.interval = Some((b, t));
            self.random.ok_or(Failure::UnsupportedCapability(
                UnsupportedCapability::RandomIntegerInterval,
            ))
        }
    }
    fn num(n: f64) -> Arg {
        Arg(Scalar::Number(n), Origin::Direct)
    }
    fn error(e: CellError) -> Arg {
        Arg(Scalar::Error(e), Origin::Reference)
    }
    #[test]
    fn excel_ceiling_policy_preserves_shared_errors_and_pinned_modes() {
        for mode in [0.1, -0.1, 1.0, -1.0] {
            let args = [num(-4.3), num(2.0), num(mode)];
            assert_eq!(
                evaluate_with_policy(
                    &mut Port::default(),
                    MathFunction::CeilingMath,
                    &args,
                    SemanticsPolicy::ExcelDocumentation
                ),
                Ok(-6.0)
            );
            assert_eq!(
                evaluate(&mut Port::default(), MathFunction::CeilingMath, &args),
                Ok(if mode < 0.0 { -6.0 } else { -4.0 })
            );
        }
        let mut p = Port::default();
        assert_eq!(
            evaluate_with_policy(
                &mut p,
                MathFunction::CeilingMath,
                &[num(-4.3), num(0.0), error(CellError::NA)],
                SemanticsPolicy::ExcelDocumentation
            ),
            Ok(0.0)
        );
        assert_eq!(p.reads, 2);
        assert_eq!(
            evaluate_with_policy(
                &mut p,
                MathFunction::CeilingMath,
                &[num(-4.3), num(2.0), error(CellError::Ref)],
                SemanticsPolicy::ExcelDocumentation
            ),
            Err(Failure::Spreadsheet(CellError::Ref))
        );
    }
    #[test]
    fn rejects_arity_before_any_evaluation_or_rng() {
        let mut p = Port::default();
        assert_eq!(
            evaluate(&mut p, MathFunction::Atan2, &[error(CellError::NA)]),
            Err(Failure::Spreadsheet(CellError::Value))
        );
        assert_eq!(
            evaluate(&mut p, MathFunction::Rand, &[num(1.0)]),
            Err(Failure::Spreadsheet(CellError::Value))
        );
        assert_eq!((p.reads, p.random_calls), (0, 0));
    }
    #[test]
    fn optional_zero_and_ignored_extras_stay_lazy() {
        let mut p = Port::default();
        assert_eq!(
            evaluate(
                &mut p,
                MathFunction::CeilingMath,
                &[num(4.3), num(0.0), error(CellError::NA)]
            ),
            Ok(0.0)
        );
        assert_eq!(p.reads, 2);
        assert_eq!(
            evaluate(
                &mut p,
                MathFunction::FloorPrecise,
                &[num(4.3), num(2.0), error(CellError::NA)]
            ),
            Ok(4.0)
        );
        assert_eq!(p.reads, 4);
        assert_eq!(
            evaluate(&mut p, MathFunction::CeilingPrecise, &[num(4.3)]),
            Ok(5.0)
        );
    }
    #[test]
    fn spreadsheet_error_collapsing_never_masks_operational_faults() {
        let mut p = Port::default();
        assert_eq!(
            evaluate(&mut p, MathFunction::Acosh, &[error(CellError::Ref)]),
            Err(Failure::Spreadsheet(CellError::Ref))
        );
        assert_eq!(
            evaluate(
                &mut p,
                MathFunction::Quotient,
                &[error(CellError::Ref), num(0.0)]
            ),
            Err(Failure::Spreadsheet(CellError::Value))
        );
        let mut p = Port {
            work_limit: Some(1),
            ..Port::default()
        };
        assert_eq!(
            evaluate(&mut p, MathFunction::Gcd, &[num(1.0)]),
            Err(Failure::Operational("work"))
        );
        assert_eq!(p.reads, 0);
    }
    #[test]
    fn collection_and_text_limits_are_checked_before_algorithms() {
        let mut p = Port {
            collection_limit: Some(1),
            ..Port::default()
        };
        assert_eq!(
            evaluate(&mut p, MathFunction::Lcm, &[num(6.0), num(8.0)]),
            Err(Failure::Operational("collection"))
        );
        let mut p = Port {
            text_limit: Some(2),
            ..Port::default()
        };
        assert_eq!(
            evaluate(
                &mut p,
                MathFunction::Fact,
                &[Arg(Scalar::Text("999".into()), Origin::Array)]
            ),
            Err(Failure::Operational("text"))
        );
    }
    #[test]
    fn scalar_origins_share_numeric_coercion_and_missing_is_explicit() {
        for origin in [Origin::Direct, Origin::Reference, Origin::Array] {
            let mut p = Port::default();
            assert_eq!(
                evaluate(
                    &mut p,
                    MathFunction::Fact,
                    &[Arg(Scalar::Boolean(true), origin)]
                ),
                Ok(1.0)
            );
            assert_eq!(
                evaluate(&mut p, MathFunction::Fact, &[Arg(Scalar::Blank, origin)]),
                Ok(1.0)
            );
        }
        assert_eq!(
            evaluate(
                &mut Port::default(),
                MathFunction::Fact,
                &[Arg(Scalar::Missing, Origin::Direct)]
            ),
            Err(Failure::UnsupportedCapability(
                UnsupportedCapability::MissingNumericArgument
            ))
        );
    }
    #[test]
    fn random_requires_actual_stream_and_preserves_bound_precedence() {
        assert_eq!(
            evaluate(&mut Port::default(), MathFunction::Rand, &[]),
            Err(Failure::UnsupportedCapability(
                UnsupportedCapability::RandomStream
            ))
        );
        let mut p = Port {
            random: Some(0.25),
            ..Port::default()
        };
        assert_eq!(evaluate(&mut p, MathFunction::Rand, &[]), Ok(0.25));
        assert_eq!(p.random_calls, 1);
        let mut p = Port {
            random: Some(2.0),
            ..Port::default()
        };
        assert_eq!(
            evaluate(&mut p, MathFunction::Randbetween, &[num(1.2), num(1.8)]),
            Ok(2.0)
        );
        assert_eq!(p.interval, Some((2.0, 2.0)));
        assert_eq!(
            evaluate(&mut p, MathFunction::Randbetween, &[num(3.0), num(2.0)]),
            Err(Failure::Spreadsheet(CellError::Num))
        );
        assert_eq!(p.random_calls, 1);
    }
    #[test]
    fn variadic_input_errors_precede_later_arguments() {
        let mut p = Port::default();
        assert_eq!(
            evaluate(
                &mut p,
                MathFunction::Gcd,
                &[num(-1.0), error(CellError::NA)]
            ),
            Err(Failure::Spreadsheet(CellError::Num))
        );
        assert_eq!(p.reads, 1);
        let mut p = Port::default();
        assert_eq!(
            evaluate(&mut p, MathFunction::Gcd, &[num(2.0), error(CellError::NA)]),
            Err(Failure::Spreadsheet(CellError::Value))
        );
    }
}
