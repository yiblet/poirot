use crate::{Collection, NumericPairs, Operand, Origin, PairCollection, Value, checked, finite};
use poirot_functions_core::CellError;

/// Row-major operands preserve source kind even after reference resolution.
pub fn collect(
    policy: Collection,
    operands: &[Operand],
) -> std::result::Result<Vec<f64>, CellError> {
    let mut values = Vec::new();
    for operand in operands {
        let referenced = operand.origin != Origin::Direct;
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
                if policy == Collection::ExtremaA {
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
pub fn collect_pairs(
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
    fn number(policy: PairCollection, value: Value) -> Option<f64> {
        match value {
            Value::Number(n) => Some(n),
            Value::Boolean(b) if policy == PairCollection::NumericCoercion => {
                Some(f64::from(u8::from(b)))
            }
            Value::Text { numeric } if policy == PairCollection::NumericCoercion => numeric,
            _ => None,
        }
    }
    let mut pairs = NumericPairs {
        x: Vec::new(),
        y: Vec::new(),
        original_len: x.len(),
    };
    for (&a, &b) in x.iter().zip(y) {
        if let (Some(a), Some(b)) = (number(policy, a), number(policy, b)) {
            finite(&[a, b])?;
            pairs.x.push(a);
            pairs.y.push(b);
        }
    }
    Ok(pairs)
}
