//! Result transfer contracts required by every builtin registration.
use crate::model::{FormulaType, ScalarKinds};

/// A builtin's possible result transformation, independent of its read effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ResultRule {
    /// Consume arguments and return a scalar of any kind.
    Scalar,
    /// Consume arguments and return a number or spreadsheet error.
    Number,
    /// Consume arguments and return complex or other scalar text.
    Text,
    /// Produce a materialized rectangular array, or an error.
    Array,
    /// Produce an array whose dimensions cannot exceed the first argument.
    InputBounds,
    /// Produce numeric array elements within the first input's dimensions.
    NumericInputBounds,
    /// Append inputs vertically, summing row bounds and joining column bounds.
    StackRows,
    /// Append inputs horizontally, joining row bounds and summing column bounds.
    StackColumns,
    /// Flatten an input into one row, bounded by its maximum element count.
    FlattenRows,
    /// Flatten an input into one column, bounded by its maximum element count.
    FlattenColumns,
    /// Select columns with possible repetition from scalar or array index arguments.
    SelectColumns,
    /// Select rows with possible repetition from scalar or array index arguments.
    SelectRows,
    /// Multiply matrices with outer dimensions supplied by the two operands.
    MatrixProduct,
    /// Select from the first input or a separately bounded empty-result fallback.
    Filter,
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
    /// Lift the first three numeric arguments while reducing later collection operands.
    FirstThreeNumericArguments,
    /// Lift two numeric controls while reducing later collection operands.
    FirstTwoNumericArguments,
    /// Lift the first numeric parameter while consuming later collection operands.
    FirstNumericArgument,
    /// Lift an optional third numeric parameter while consuming earlier collections.
    ThirdNumericArgument,
    /// Lift only the second scalar argument, reducing the first collection.
    SecondNumericArgument,
    /// Lift alternating criteria after each criteria range.
    PairedCriteria,
    /// Lift alternating criteria after a leading payload range.
    PayloadCriteria,
    /// Lift scalar parameters after the first collection.
    AfterFirstNumericArguments,
    /// Lift scalar parameters after two collections.
    AfterSecondNumericArguments,
    /// Lift forecast target and options, excluding the values and timeline collections.
    ForecastArguments,
    /// Lift rank target and optional order, reducing the second argument.
    RankArguments,
    /// Produce a square numeric identity matrix from one dimension.
    UnitMatrix,
    /// Return text with lifting driven by the input arguments.
    ElementwiseText,
    /// Extract text or preserve the optional sixth fallback argument's scalar kinds.
    TextWithFallback,
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
    pub fn apply(self, args: &[FormulaType]) -> FormulaType {
        let all = args
            .iter()
            .copied()
            .fold(FormulaType::BOTTOM, FormulaType::join);
        let error = FormulaType::scalar(ScalarKinds::ERROR);
        let selected = |indices: &dyn Fn(usize) -> bool| {
            args.iter()
                .enumerate()
                .filter(|(i, _)| indices(*i))
                .map(|(_, t)| *t)
                .fold(FormulaType::BOTTOM, FormulaType::join)
        };
        let lift = |result: FormulaType| {
            if all.may_spill() {
                result.join(FormulaType::array(
                    result.elements().join(ScalarKinds::ERROR),
                ))
            } else {
                result
            }
        };
        match self {
            Self::FirstTwoNumericArguments => {
                Self::ElementwiseNumber.apply(args.get(..2).unwrap_or(args))
            }
            Self::FirstThreeNumericArguments => {
                Self::ElementwiseNumber.apply(args.get(..3).unwrap_or(args))
            }
            Self::FirstNumericArgument => {
                Self::ElementwiseNumber.apply(args.get(..1).unwrap_or(args))
            }
            Self::ThirdNumericArgument => {
                Self::ElementwiseNumber.apply(args.get(2..3).unwrap_or(&[]))
            }
            Self::SecondNumericArgument => {
                Self::ElementwiseNumber.apply(args.get(1..2).unwrap_or(&[]))
            }
            Self::PairedCriteria => Self::ElementwiseNumber.apply(&[selected(&|i| i % 2 == 1)]),
            Self::PayloadCriteria => {
                Self::ElementwiseNumber.apply(&[selected(&|i| i >= 2 && i.is_multiple_of(2))])
            }
            Self::AfterFirstNumericArguments => {
                Self::ElementwiseNumber.apply(args.get(1..).unwrap_or(&[]))
            }
            Self::AfterSecondNumericArguments => {
                Self::ElementwiseNumber.apply(args.get(2..).unwrap_or(&[]))
            }
            Self::ForecastArguments => {
                let first = args.first().copied().unwrap_or(FormulaType::BOTTOM);
                Self::ElementwiseNumber.apply(&[first, selected(&|i| i >= 3)])
            }
            Self::RankArguments => Self::ElementwiseNumber.apply(&[
                args.first().copied().unwrap_or(FormulaType::BOTTOM),
                args.get(2).copied().unwrap_or(FormulaType::BOTTOM),
            ]),
            Self::Scalar => FormulaType::scalar(ScalarKinds::ANY),
            Self::Number => FormulaType::scalar(ScalarKinds::NUMBER.join(ScalarKinds::ERROR)),
            Self::Text => FormulaType::scalar(ScalarKinds::TEXT.join(ScalarKinds::ERROR)),
            Self::Array => FormulaType::array(ScalarKinds::ANY).join(error),
            Self::Sequence | Self::MatrixProduct | Self::NumericInputBounds => {
                FormulaType::array(ScalarKinds::NUMBER.join(ScalarKinds::ERROR)).join(error)
            }
            Self::InputBounds
            | Self::FlattenRows
            | Self::FlattenColumns
            | Self::SelectRows
            | Self::SelectColumns
            | Self::Transpose => FormulaType::array(materialized_kinds(
                args.first().copied().unwrap_or(FormulaType::BOTTOM),
            ))
            .join(error),
            Self::StackRows | Self::StackColumns => {
                FormulaType::array(materialized_kinds(all)).join(error)
            }
            Self::Filter => {
                let input = args.first().copied().unwrap_or(FormulaType::BOTTOM);
                let fallback = args.get(2).copied().unwrap_or(FormulaType::BOTTOM);
                FormulaType::array(materialized_kinds(input.join(fallback))).join(error)
            }
            Self::RandomArray | Self::UnitMatrix => {
                FormulaType::array(ScalarKinds::NUMBER).join(error)
            }
            Self::TrimRange => {
                let first = args.first().copied().unwrap_or(FormulaType::BOTTOM);
                let mut result = error;
                if !first.single_reference.is_empty() || !first.multi_reference.is_empty() {
                    result = result
                        .join(FormulaType::single_reference(ScalarKinds::ANY))
                        .join(FormulaType::multi_reference(ScalarKinds::ANY));
                }
                if !first.array.is_empty() || !first.scalar.is_empty() {
                    result = result.join(FormulaType::array(ScalarKinds::ANY));
                }
                result
            }
            Self::TextWithFallback => {
                let mut kinds = ScalarKinds::TEXT.join(ScalarKinds::ERROR);
                if let Some(fallback) = args.get(5) {
                    kinds = kinds.join(fallback.elements());
                    if fallback.missing || ScalarKinds::BLANK.is_subtype_of(fallback.elements()) {
                        kinds = kinds.join(ScalarKinds::NUMBER);
                    }
                }
                let scalar = FormulaType::scalar(kinds);
                if all.may_spill() {
                    scalar.join(FormulaType::array(kinds))
                } else {
                    scalar
                }
            }
            Self::RegexExtract => FormulaType::scalar(ScalarKinds::TEXT.join(ScalarKinds::ERROR))
                .join(FormulaType::array(
                    ScalarKinds::TEXT.join(ScalarKinds::ERROR),
                )),
            Self::ElementwiseBoolean | Self::ElementwiseText | Self::ElementwiseNumber => {
                let kinds = if self == Self::ElementwiseBoolean {
                    ScalarKinds::BOOLEAN
                } else if self == Self::ElementwiseNumber {
                    ScalarKinds::NUMBER
                } else {
                    ScalarKinds::TEXT
                };
                let scalar = FormulaType::scalar(kinds.join(ScalarKinds::ERROR));
                if all.may_spill() {
                    scalar.join(FormulaType::array(kinds.join(ScalarKinds::ERROR)))
                } else {
                    scalar
                }
            }
            Self::If => lift(selected(&|i| i == 1 || i == 2).join(FormulaType::scalar(
                ScalarKinds::BOOLEAN.join(ScalarKinds::BLANK),
            )))
            .join(error),
            Self::Fallback => lift(all).join(error),
            Self::Ifs => lift(selected(&|i| i % 2 == 1)).join(error),
            Self::Switch => lift(selected(&|i| {
                i >= 2 && i.is_multiple_of(2)
                    || i.checked_add(1) == Some(args.len()) && args.len().is_multiple_of(2)
            }))
            .join(error),
            Self::Choose => lift(selected(&|i| i >= 1)).join(error),
            Self::SpillRange => FormulaType::multi_reference(ScalarKinds::ANY).join(error),
            Self::Reference => FormulaType::single_reference(ScalarKinds::ANY)
                .join(FormulaType::multi_reference(ScalarKinds::ANY))
                .join(error),
            Self::Index => {
                // INDEX preserves the reference/value channel of its input; scalar
                // and singleton array outcomes are included conservatively.
                let mut result = FormulaType::scalar(ScalarKinds::ANY).join(error);
                if !all.array.is_empty() {
                    result = result.join(FormulaType::array(ScalarKinds::ANY));
                }
                if !all.single_reference.is_empty() || !all.multi_reference.is_empty() {
                    result = result
                        .join(FormulaType::single_reference(ScalarKinds::ANY))
                        .join(FormulaType::multi_reference(ScalarKinds::ANY));
                }
                result
            }
            Self::Any => FormulaType::TOP,
        }
    }
}

impl ResultRule {
    /// Transfer independent geometry proofs through the same catalog result contract.
    /// Constant arguments are numeric literals only; other values remain unknown.
    pub(in crate::eval) fn shape(
        self,
        args: &[crate::model::ShapeBounds],
        constants: &[Option<f64>],
    ) -> crate::model::ShapeBounds {
        use crate::model::ShapeBounds;
        match self {
            Self::FirstTwoNumericArguments => args
                .iter()
                .take(2)
                .copied()
                .fold(ShapeBounds::SCALAR, ShapeBounds::join),
            Self::FirstThreeNumericArguments => args
                .iter()
                .take(3)
                .copied()
                .fold(ShapeBounds::SCALAR, ShapeBounds::join),
            Self::FirstNumericArgument => args.first().copied().unwrap_or(ShapeBounds::SCALAR),
            Self::ThirdNumericArgument => args.get(2).copied().unwrap_or(ShapeBounds::SCALAR),
            Self::SecondNumericArgument => args.get(1).copied().unwrap_or(ShapeBounds::SCALAR),
            Self::PairedCriteria | Self::PayloadCriteria => args
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    if self == Self::PairedCriteria {
                        i % 2 == 1
                    } else {
                        *i >= 2 && i.is_multiple_of(2)
                    }
                })
                .map(|(_, shape)| *shape)
                .fold(ShapeBounds::SCALAR, ShapeBounds::join),
            Self::AfterFirstNumericArguments => args
                .iter()
                .skip(1)
                .copied()
                .fold(ShapeBounds::SCALAR, ShapeBounds::join),
            Self::AfterSecondNumericArguments => args
                .iter()
                .skip(2)
                .copied()
                .fold(ShapeBounds::SCALAR, ShapeBounds::join),
            Self::ForecastArguments => args
                .iter()
                .enumerate()
                .filter(|(i, _)| *i == 0 || *i >= 3)
                .map(|(_, s)| *s)
                .fold(ShapeBounds::SCALAR, ShapeBounds::join),
            Self::RankArguments => args
                .first()
                .copied()
                .unwrap_or(ShapeBounds::SCALAR)
                .join(args.get(2).copied().unwrap_or(ShapeBounds::SCALAR)),
            Self::Scalar | Self::Number | Self::Text => ShapeBounds::SCALAR,
            Self::InputBounds | Self::NumericInputBounds => {
                args.first().copied().unwrap_or(ShapeBounds::UNKNOWN)
            }
            Self::StackRows | Self::StackColumns => {
                let joined = args
                    .iter()
                    .copied()
                    .fold(ShapeBounds::SCALAR, ShapeBounds::join);
                let summed = sum_dimensions(args.iter().map(|shape| {
                    if self == Self::StackRows {
                        shape.rows
                    } else {
                        shape.columns
                    }
                }));
                if self == Self::StackRows {
                    ShapeBounds {
                        rows: summed,
                        columns: joined.columns,
                    }
                } else {
                    ShapeBounds {
                        rows: joined.rows,
                        columns: summed,
                    }
                }
            }
            Self::FlattenRows | Self::FlattenColumns => {
                let bound = args.first().copied().unwrap_or(ShapeBounds::UNKNOWN);
                let count = element_bound(bound);
                if self == Self::FlattenRows {
                    ShapeBounds {
                        rows: ShapeBounds::SCALAR.rows,
                        columns: count,
                    }
                } else {
                    ShapeBounds {
                        rows: count,
                        columns: ShapeBounds::SCALAR.columns,
                    }
                }
            }
            Self::SelectRows | Self::SelectColumns => {
                let input = args.first().copied().unwrap_or(ShapeBounds::UNKNOWN);
                let count = sum_dimensions(args.iter().skip(1).copied().map(element_bound));
                if self == Self::SelectRows {
                    ShapeBounds {
                        rows: count,
                        columns: input.columns,
                    }
                } else {
                    ShapeBounds {
                        rows: input.rows,
                        columns: count,
                    }
                }
            }
            Self::MatrixProduct => ShapeBounds {
                rows: args.first().copied().unwrap_or(ShapeBounds::UNKNOWN).rows,
                columns: args.get(1).copied().unwrap_or(ShapeBounds::UNKNOWN).columns,
            },
            Self::Filter => args
                .first()
                .copied()
                .unwrap_or(ShapeBounds::UNKNOWN)
                .join(args.get(2).copied().unwrap_or(ShapeBounds::SCALAR)),
            Self::Transpose => args
                .first()
                .copied()
                .unwrap_or(ShapeBounds::UNKNOWN)
                .transpose(),
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
                ShapeBounds {
                    rows: dimension(0),
                    columns: dimension(1),
                }
            }
            Self::UnitMatrix => {
                let size = constants.first().copied().flatten().and_then(|n| {
                    if n.is_finite() && n >= 1.0 && n <= f64::from(crate::model::MAX_COLUMNS) {
                        std::num::NonZeroU32::new(n.trunc() as u32)
                    } else {
                        None
                    }
                });
                ShapeBounds {
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
                    Some(0.0) => args
                        .iter()
                        .copied()
                        .fold(ShapeBounds::SCALAR, ShapeBounds::join),
                    Some(1.0) if args.iter().all(|s| *s == ShapeBounds::SCALAR) => ShapeBounds {
                        rows: None,
                        columns: ShapeBounds::SCALAR.columns,
                    },
                    Some(2.0) if args.iter().all(|s| *s == ShapeBounds::SCALAR) => ShapeBounds {
                        rows: ShapeBounds::SCALAR.rows,
                        columns: None,
                    },
                    _ => ShapeBounds::UNKNOWN,
                }
            }
            Self::ElementwiseNumber
            | Self::ElementwiseBoolean
            | Self::ElementwiseText
            | Self::TextWithFallback
            | Self::If
            | Self::Fallback
            | Self::Ifs
            | Self::Switch
            | Self::Choose => args
                .iter()
                .copied()
                .fold(ShapeBounds::SCALAR, ShapeBounds::join),
            Self::Array
            | Self::TrimRange
            | Self::Reference
            | Self::SpillRange
            | Self::Index
            | Self::Any => ShapeBounds::UNKNOWN,
        }
    }
}

/// Bound a flattened rectangular operand, refusing unknown or overflowing products.
fn element_bound(shape: crate::model::ShapeBounds) -> Option<std::num::NonZeroU32> {
    shape
        .rows?
        .get()
        .checked_mul(shape.columns?.get())
        .and_then(std::num::NonZeroU32::new)
}

/// Sum independent maximum extents; an unknown or overflowing term stays unknown.
fn sum_dimensions(
    dimensions: impl Iterator<Item = Option<std::num::NonZeroU32>>,
) -> Option<std::num::NonZeroU32> {
    dimensions
        .map(|n| n.map(std::num::NonZeroU32::get))
        .try_fold(0u32, |sum, n| sum.checked_add(n?))
        .and_then(std::num::NonZeroU32::new)
}

/// Preserve data element kinds while admitting blank materialization and positional errors.
fn materialized_kinds(input: FormulaType) -> ScalarKinds {
    let kinds = input.elements().join(ScalarKinds::ERROR);
    if input.missing || ScalarKinds::BLANK.is_subtype_of(kinds) {
        kinds.join(ScalarKinds::NUMBER)
    } else {
        kinds
    }
}
