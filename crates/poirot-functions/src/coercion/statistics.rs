//! Origin-sensitive statistical input contracts.
use crate::scalar::CellError;
/// Text parsing is performed by the adapter's Excel numeric parser, never by this crate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    /// Numeric input before statistical collection.
    Number(f64),
    /// Boolean input whose admission depends on origin and collection mode.
    Boolean(bool),
    /// Text with optional adapter-parsed numeric interpretation.
    Text {
        /// Numeric interpretation supplied by the adapter, if parsing succeeded.
        numeric: Option<f64>,
    },
    /// Blank input, distinct from an omitted argument.
    Blank,
    /// Omitted argument slot.
    Missing,
    /// Spreadsheet error retained until collection applies error precedence.
    Error(CellError),
}
/// Operand provenance controlling statistical text, boolean, and blank conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Direct scalar expression or literal.
    Direct,
    /// Materialized scalar selected from another operand. Its blank is ignored,
    /// while logical/text admission follows the scalar argument channel.
    SelectedScalar,
    /// Value selected from a worksheet reference.
    Reference,
    /// Member selected from a materialized array.
    Array,
}
/// A statistical value with its direct/reference/array origin. Origin controls
/// whether text, booleans, and blanks are ignored, converted, or rejected.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Operand {
    /// Uncoerced value admitted by the host.
    pub value: Value,
    /// Provenance used to select collection rules.
    pub origin: Origin,
}
/// Value-admission rules selected before a statistical aggregate runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Collection {
    /// Ordinary numeric aggregate conversion and reference filtering.
    NumericAggregate,
    /// A-family aggregation admitting booleans and text as specified.
    AggregateA,
    /// A-family minimum/maximum collection rules.
    ExtremaA,
    /// Collection rules for geometric mean.
    GeoMean,
    /// Collection rules for most-frequent numeric value.
    Mode,
    /// Collection rules for percentile-rank input.
    PercentRank,
    /// Collection rules for ranking a value within a sample.
    Rank,
}

/// Admission rule for positionally aligned X/Y operands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairCollection {
    /// Retain pairs whose members are both numeric.
    NumericOnly,
    /// Apply numeric conversion before retaining aligned pairs.
    NumericCoercionV2,
}
/// Positionally matched numeric X/Y vectors after pair filtering.
/// `original_len` preserves the pre-filter length for source algorithms whose
/// denominator includes skipped positions.
#[derive(Clone, Debug, PartialEq)]
pub struct NumericPairs {
    /// Filtered X values, positionally aligned with Y.
    pub x: Vec<f64>,
    /// Filtered Y values, positionally aligned with X.
    pub y: Vec<f64>,
    /// Input pair count before filtering.
    pub original_len: usize,
}

impl NumericPairs {
    /// Number of aligned observations admitted to this sample, distinct from its input shape.
    pub fn sample_len(&self) -> Result<usize, CellError> {
        if self.x.len() != self.y.len() || self.x.len() > self.original_len {
            Err(CellError::NA)
        } else {
            Ok(self.x.len())
        }
    }
}
/// Admit an aligned observation only when both coordinates are already numbers.
/// Error precedence belongs to the acquisition protocol before this membership test.
/// Numeric-looking text and booleans remain excluded; numeric zero remains included.
pub fn coercev2_statistics_numeric_pair(left: Value, right: Value) -> Option<(f64, f64)> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => Some((left, right)),
        _ => None,
    }
}

fn checked(value: f64) -> Result<f64, CellError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(CellError::Num)
    }
}
fn finite(values: &[f64]) -> Result<(), CellError> {
    values.iter().try_for_each(|v| checked(*v).map(|_| ()))
}
/// Row-major operands preserve source kind even after reference resolution.
pub fn coercev2_statistics_collection(
    policy: Collection,
    operands: &[Operand],
) -> std::result::Result<Vec<f64>, CellError> {
    let mut values = Vec::new();
    for operand in operands {
        let referenced = matches!(operand.origin, Origin::Reference | Origin::Array);
        let value = match operand.value {
            Value::Number(n) => Some(n),
            Value::Error(e) => {
                if policy == Collection::Rank {
                    None
                } else {
                    return Err(if policy == Collection::PercentRank && e == CellError::NA {
                        CellError::Num
                    } else {
                        e
                    });
                }
            }
            Value::Boolean(b) => match policy {
                Collection::Rank => None,
                Collection::Mode => {
                    if operand.origin == Origin::Array {
                        None
                    } else {
                        return Err(CellError::Value);
                    }
                }
                Collection::AggregateA | Collection::ExtremaA | Collection::PercentRank => {
                    Some(f64::from(u8::from(b)))
                }
                _ => {
                    if referenced {
                        None
                    } else {
                        Some(f64::from(u8::from(b)))
                    }
                }
            },
            Value::Text { numeric } => match policy {
                Collection::Rank => None,
                Collection::Mode => {
                    if operand.origin == Origin::Array {
                        None
                    } else {
                        return Err(CellError::Value);
                    }
                }
                Collection::AggregateA => Some(0.0),
                Collection::ExtremaA if referenced => Some(0.0),
                Collection::PercentRank => Some(numeric.ok_or(CellError::Value)?),
                _ => {
                    if referenced {
                        None
                    } else {
                        Some(numeric.ok_or(CellError::Value)?)
                    }
                }
            },
            Value::Blank => {
                if operand.origin == Origin::Direct
                    && matches!(
                        policy,
                        Collection::NumericAggregate
                            | Collection::AggregateA
                            | Collection::ExtremaA
                            | Collection::GeoMean
                    )
                {
                    values.push(0.0);
                    continue;
                }
                if policy == Collection::Mode && operand.origin != Origin::Array {
                    return Err(CellError::Value);
                }
                None
            }
            Value::Missing => {
                if policy == Collection::GeoMean {
                    Some(0.0)
                } else {
                    None
                }
            }
        };
        if let Some(value) = value {
            checked(value)?;
            values.push(value);
        }
    }
    Ok(values)
}
/// Fully inspect X before Y, matching POI's pair error precedence.
pub fn coercev2_statistics_pairs(
    policy: PairCollection,
    x: &[Value],
    y: &[Value],
) -> std::result::Result<NumericPairs, CellError> {
    if x.len() != y.len() {
        return Err(CellError::NA);
    }
    for value in x.iter().chain(y) {
        if let Value::Error(error) = value {
            return Err(*error);
        }
    }
    fn coercev2_pair_number(policy: PairCollection, value: Value) -> Option<f64> {
        match value {
            Value::Number(n) => Some(n),
            Value::Boolean(b) if policy == PairCollection::NumericCoercionV2 => {
                Some(f64::from(u8::from(b)))
            }
            Value::Text { numeric } if policy == PairCollection::NumericCoercionV2 => numeric,
            _ => None,
        }
    }
    let mut pairs = NumericPairs {
        x: Vec::new(),
        y: Vec::new(),
        original_len: x.len(),
    };
    for (&a, &b) in x.iter().zip(y) {
        if let (Some(a), Some(b)) = (
            coercev2_pair_number(policy, a),
            coercev2_pair_number(policy, b),
        ) {
            finite(&[a, b])?;
            pairs.x.push(a);
            pairs.y.push(b);
        }
    }
    Ok(pairs)
}
