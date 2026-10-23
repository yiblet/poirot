//! Result transfer contracts required by every builtin registration.
use poirot_model::{FormulaType as T, ScalarKinds as K};

/// A builtin's possible result transformation, independent of its read effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultRule {
    /// Consume arguments and return a scalar of any kind.
    Scalar,
    /// Consume arguments and return a number or spreadsheet error.
    Number,
    /// Produce a materialized rectangular array, or an error.
    Array,
    /// Produce sequential values with independently specified dimensions.
    Sequence,
    /// Exchange the rows and columns of the first operand.
    Transpose,
    /// Inspect elements, preserving possible array lifting.
    ElementwiseBoolean,
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
            Self::Scalar => T::scalar(K::ANY),
            Self::Number => T::scalar(K::NUMBER.join(K::ERROR)),
            Self::Array | Self::Sequence | Self::Transpose => T::array(K::ANY).join(error),
            Self::ElementwiseBoolean => {
                let scalar = T::scalar(K::BOOLEAN.join(K::ERROR));
                if all.may_spill() {
                    scalar.join(T::array(K::BOOLEAN.join(K::ERROR)))
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
            Self::Scalar | Self::Number => S::SCALAR,
            Self::Transpose => args.first().copied().unwrap_or(S::UNKNOWN).transpose(),
            Self::Sequence => {
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
            Self::ElementwiseBoolean
            | Self::If
            | Self::Fallback
            | Self::Ifs
            | Self::Switch
            | Self::Choose => args.iter().copied().fold(S::SCALAR, S::join),
            Self::Array | Self::Reference | Self::SpillRange | Self::Index | Self::Any => {
                S::UNKNOWN
            }
        }
    }
}
