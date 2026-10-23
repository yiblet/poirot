//! Result transfer contracts required by every builtin registration.
use poirot_model::{FormulaType as T, ScalarKinds as K};

/// A builtin's possible result transformation, independent of its read effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultRule {
    /// Consume arguments and return a scalar of any kind.
    Scalar,
    /// Consume arguments and return a number or spreadsheet error.
    Number,
    /// Consume arguments and return complex or other scalar text.
    Text,
    /// Produce a materialized rectangular array, or an error.
    Array,
    /// Produce sequential values with independently specified dimensions.
    Sequence,
    /// Produce random numbers with optional independently specified dimensions.
    RandomArray,
    /// Crop references as references and other operands as materialized arrays.
    TrimRange,
    /// Exchange the rows and columns of the first operand.
    Transpose,
    /// Inspect elements, preserving possible array lifting.
    ElementwiseBoolean,
    /// Return numbers or errors with rectangular scalar-argument lifting.
    ElementwiseNumber,
    /// Lift business-day endpoints/weekend options while reducing the holiday set.
    BusinessDays,
    /// Lift the first numeric parameter while consuming later collection operands.
    FirstNumericArgument,
    /// Lift an optional third numeric parameter while consuming earlier collections.
    ThirdNumericArgument,
    /// Produce a square numeric identity matrix from one dimension.
    UnitMatrix,
    /// Return text with lifting driven by the input arguments.
    ElementwiseText,
    /// Return regex match text, including variable-length extraction arrays.
    RegexExtract,
    /// Select IF branches, with lifting driven by its condition and branches.
    If,
    /// Select an original value or fallback, preserving array lifting.
    Fallback,
    /// Select alternating condition/result pairs.
    Ifs,
    /// Select matching result pairs or a default.
    Switch,
    /// Select indexed alternatives, including array index lifting.
    Choose,
    /// Produce a dynamically selected reference or spreadsheet error.
    Reference,
    /// Accepted spill footprint as a potentially singleton area reference.
    SpillRange,
    /// Select from a reference or materialized array.
    Index,
    /// A handler whose possible successful result requires every form.
    Any,
}
impl ResultRule {
    /// Apply the contract to possible argument types; invalid arities may still yield errors.
    pub fn apply(self, args: &[T]) -> T {
        let all = args.iter().copied().fold(T::BOTTOM, T::join);
        let error = T::scalar(K::ERROR);
        let selected = |indices: &dyn Fn(usize) -> bool| {
            args.iter()
                .enumerate()
                .filter(|(i, _)| indices(*i))
                .map(|(_, t)| *t)
                .fold(T::BOTTOM, T::join)
        };
        let lift = |result: T| {
            if all.may_spill() {
                result.join(T::array(result.elements().join(K::ERROR)))
            } else {
                result
            }
        };
        match self {
            Self::BusinessDays => Self::ElementwiseNumber.apply(args.get(..3).unwrap_or(args)),
            Self::FirstNumericArgument => {
                Self::ElementwiseNumber.apply(args.get(..1).unwrap_or(args))
            }
            Self::ThirdNumericArgument => {
                Self::ElementwiseNumber.apply(args.get(2..3).unwrap_or(&[]))
            }
            Self::Scalar => T::scalar(K::ANY),
            Self::Number => T::scalar(K::NUMBER.join(K::ERROR)),
            Self::Text => T::scalar(K::TEXT.join(K::ERROR)),
            Self::Array | Self::Sequence | Self::Transpose => T::array(K::ANY).join(error),
            Self::RandomArray | Self::UnitMatrix => T::array(K::NUMBER).join(error),
            Self::TrimRange => {
                let first = args.first().copied().unwrap_or(T::BOTTOM);
                let mut result = error;
                if !first.single_reference.is_empty() || !first.multi_reference.is_empty() {
                    result = result
                        .join(T::single_reference(K::ANY))
                        .join(T::multi_reference(K::ANY));
                }
                if !first.array.is_empty() || !first.scalar.is_empty() {
                    result = result.join(T::array(K::ANY));
                }
                result
            }
            Self::RegexExtract => {
                T::scalar(K::TEXT.join(K::ERROR)).join(T::array(K::TEXT.join(K::ERROR)))
            }
            Self::ElementwiseBoolean | Self::ElementwiseText | Self::ElementwiseNumber => {
                let kinds = if self == Self::ElementwiseBoolean {
                    K::BOOLEAN
                } else if self == Self::ElementwiseNumber {
                    K::NUMBER
                } else {
                    K::TEXT
                };
                let scalar = T::scalar(kinds.join(K::ERROR));
                if all.may_spill() {
                    scalar.join(T::array(kinds.join(K::ERROR)))
                } else {
                    scalar
                }
            }
            Self::If => {
                lift(selected(&|i| i == 1 || i == 2).join(T::scalar(K::BOOLEAN.join(K::BLANK))))
                    .join(error)
            }
            Self::Fallback => lift(all).join(error),
            Self::Ifs => lift(selected(&|i| i % 2 == 1)).join(error),
            Self::Switch => lift(selected(&|i| {
                i >= 2 && i.is_multiple_of(2)
                    || i.checked_add(1) == Some(args.len()) && args.len().is_multiple_of(2)
            }))
            .join(error),
            Self::Choose => lift(selected(&|i| i >= 1)).join(error),
            Self::SpillRange => T::multi_reference(K::ANY).join(error),
            Self::Reference => T::single_reference(K::ANY)
                .join(T::multi_reference(K::ANY))
                .join(error),
            Self::Index => {
                // INDEX preserves the reference/value channel of its input; scalar
                // and singleton array outcomes are included conservatively.
                let mut result = T::scalar(K::ANY).join(error);
                if !all.array.is_empty() {
                    result = result.join(T::array(K::ANY));
                }
                if !all.single_reference.is_empty() || !all.multi_reference.is_empty() {
                    result = result
                        .join(T::single_reference(K::ANY))
                        .join(T::multi_reference(K::ANY));
                }
                result
            }
            Self::Any => T::TOP,
        }
    }
}

impl ResultRule {
    /// Transfer independent geometry proofs through the same catalog result contract.
    /// Constant arguments are numeric literals only; other values remain unknown.
    pub(crate) fn shape(
        self,
        args: &[poirot_model::ShapeBounds],
        constants: &[Option<f64>],
    ) -> poirot_model::ShapeBounds {
        use poirot_model::ShapeBounds as S;
        match self {
            Self::BusinessDays => args.iter().take(3).copied().fold(S::SCALAR, S::join),
            Self::FirstNumericArgument => args.first().copied().unwrap_or(S::SCALAR),
            Self::ThirdNumericArgument => args.get(2).copied().unwrap_or(S::SCALAR),
            Self::Scalar | Self::Number | Self::Text => S::SCALAR,
            Self::Transpose => args.first().copied().unwrap_or(S::UNKNOWN).transpose(),
            Self::Sequence | Self::RandomArray => {
                let dimension = |index: usize| {
                    constants
                        .get(index)
                        .copied()
                        .unwrap_or(Some(1.0))
                        .and_then(|n| {
                            if n.is_finite() && n >= 1.0 && n < f64::from(u32::MAX) + 1.0 {
                                std::num::NonZeroU32::new(n.trunc() as u32)
                            } else {
                                None
                            }
                        })
                };
                S {
                    rows: dimension(0),
                    columns: dimension(1),
                }
            }
            Self::UnitMatrix => {
                let size = constants.first().copied().flatten().and_then(|n| {
                    if n.is_finite() && n >= 1.0 && n <= f64::from(poirot_model::MAX_COLUMNS) {
                        std::num::NonZeroU32::new(n.trunc() as u32)
                    } else {
                        None
                    }
                });
                S {
                    rows: size,
                    columns: size,
                }
            }
            Self::RegexExtract => {
                match constants
                    .get(2)
                    .copied()
                    .flatten()
                    .or_else(|| (args.len() <= 2).then_some(0.0))
                {
                    Some(0.0) => args.iter().copied().fold(S::SCALAR, S::join),
                    Some(1.0) if args.iter().all(|s| *s == S::SCALAR) => S {
                        rows: None,
                        columns: S::SCALAR.columns,
                    },
                    Some(2.0) if args.iter().all(|s| *s == S::SCALAR) => S {
                        rows: S::SCALAR.rows,
                        columns: None,
                    },
                    _ => S::UNKNOWN,
                }
            }
            Self::ElementwiseNumber
            | Self::ElementwiseBoolean
            | Self::ElementwiseText
            | Self::If
            | Self::Fallback
            | Self::Ifs
            | Self::Switch
            | Self::Choose => args.iter().copied().fold(S::SCALAR, S::join),
            Self::Array
            | Self::TrimRange
            | Self::Reference
            | Self::SpillRange
            | Self::Index
            | Self::Any => S::UNKNOWN,
        }
    }
}
