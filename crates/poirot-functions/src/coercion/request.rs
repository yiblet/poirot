use crate::coercion::{CoercionContext, CoercionError, CoercionMismatchError, Input};

/// A named scalar rule with a statically known output, possibly borrowed.
pub trait Coerce<'a> {
    /// Converted value; it may borrow storage from the input lifetime.
    type Output;

    /// Apply this rule, distinguishing mismatch from terminal worksheet or resource failure.
    /// Prefer [`coerce`] when invoking a rule so entry work is charged.
    fn coerce_input(
        input: Input<'a>,
        context: &mut CoercionContext,
    ) -> Result<Self::Output, CoercionError>;
}

/// A whole argument signature, rather than independent per-argument alternatives.
pub trait Signature<'a> {
    /// Typed outputs for the complete accepted argument list.
    type Output;

    /// Convert the whole list or return an arity/input mismatch or terminal failure.
    /// Prefer [`coerce_args`] when invoking a signature so entry work is charged.
    fn coerce_inputs(
        inputs: &[Input<'a>],
        context: &mut CoercionContext,
    ) -> Result<Self::Output, CoercionError>;
}

/// Request one scalar rule, charging one work unit before its own conversion charges.
/// Existing values are never rewritten by an attempt.
pub fn coerce<'a, R: Coerce<'a>>(
    input: Input<'a>,
    context: &mut CoercionContext,
) -> Result<R::Output, CoercionError> {
    context.charge(1)?;
    R::coerce_input(input, context)
}

/// Request a typed tuple or an ordered signature choice, charging one entry work unit.
/// Tuple signatures support zero through eight arguments with exact arity; represent
/// an omitted slot with [`Input::missing`] rather than shortening the argument list.
pub fn coerce_args<'a, S: Signature<'a>>(
    inputs: &[Input<'a>],
    context: &mut CoercionContext,
) -> Result<S::Output, CoercionError> {
    context.charge(1)?;
    S::coerce_inputs(inputs, context)
}

/// A statically typed choice of complete signatures, tried left to right.
/// Only [`CoercionError::Mismatch`] selects the next candidate; worksheet errors,
/// unsupported text and resource failures stop the choice. Charges from abandoned
/// candidates remain in the shared context. Nest this combinator for more alternatives.
pub struct FirstOf<S>(std::marker::PhantomData<S>);

/// The signature that matched, carrying its typed outputs.
#[derive(Debug, PartialEq)]
pub enum Choice<A, B> {
    /// Outputs of the first candidate, which accepted the entire argument list.
    First(A),
    /// Outputs of the second candidate, after the first returned a mismatch.
    Second(B),
}

impl<'a, A: Signature<'a>, B: Signature<'a>> Signature<'a> for FirstOf<(A, B)> {
    type Output = Choice<A::Output, B::Output>;

    fn coerce_inputs(
        inputs: &[Input<'a>],
        context: &mut CoercionContext,
    ) -> Result<Self::Output, CoercionError> {
        match coerce_args::<A>(inputs, context) {
            Ok(value) => Ok(Choice::First(value)),
            Err(CoercionError::Mismatch(_)) => {
                coerce_args::<B>(inputs, context).map(Choice::Second)
            }
            Err(failure) => Err(failure),
        }
    }
}

impl Signature<'_> for () {
    type Output = ();

    fn coerce_inputs(inputs: &[Input<'_>], _: &mut CoercionContext) -> Result<(), CoercionError> {
        if inputs.is_empty() {
            Ok(())
        } else {
            Err(CoercionError::Mismatch(CoercionMismatchError::Arity {
                expected: 0,
                actual: inputs.len(),
            }))
        }
    }
}

macro_rules! tuple_signature {
    ($count:expr; $( $rule:ident $value:ident $index:expr ),*) => {
        impl<'a, $( $rule: Coerce<'a> ),*> Signature<'a> for ($( $rule, )*) {
            type Output = ($( $rule::Output, )*);

            fn coerce_inputs(
                inputs: &[Input<'a>],
                context: &mut CoercionContext,
            ) -> Result<Self::Output, CoercionError> {
                let [$( $value, )*] = inputs else {
                    return Err(CoercionError::Mismatch(CoercionMismatchError::Arity {
                        expected: $count,
                        actual: inputs.len(),
                    }));
                };
                Ok(($( coerce::<$rule>(*$value, context)
                    .map_err(|error| error.at_argument($index))?, )*))
            }
        }
    };
}

tuple_signature!(1; A a 0);
tuple_signature!(2; A a 0, B b 1);
tuple_signature!(3; A a 0, B b 1, C c 2);
tuple_signature!(4; A a 0, B b 1, C c 2, D d 3);
tuple_signature!(5; A a 0, B b 1, C c 2, D d 3, E e 4);
tuple_signature!(6; A a 0, B b 1, C c 2, D d 3, E e 4, F f 5);
tuple_signature!(7; A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6);
tuple_signature!(8; A a 0, B b 1, C c 2, D d 3, E e 4, F f 5, G g 6, H h 7);
