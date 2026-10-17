//! Collection provenance and the distinct sequential and numeric-only pair protocols.
use super::coercionv2::{coercev2_statistics_decimal_text, scalar, text_check, work};
use super::{EvaluationPort, Failure, Origin, Scalar};
use poirot_functions_statistics as domain;
use poirot_model::CellError;

fn coercev2_statistics_collection_member<P: EvaluationPort>(
    p: &mut P,
    s: Scalar,
    parse: bool,
) -> Result<domain::Value, Failure<P::Operational>> {
    Ok(match s {
        Scalar::Number(n) => domain::Value::Number(n),
        Scalar::Boolean(b) => domain::Value::Boolean(b),
        Scalar::Blank => domain::Value::Blank,
        Scalar::Missing => domain::Value::Missing,
        Scalar::Error(e) => domain::Value::Error(e),
        Scalar::Text(t) => {
            let numeric = if parse {
                text_check(p, &t)?;
                coercev2_statistics_decimal_text(&t)
            } else {
                None
            };
            domain::Value::Text { numeric }
        }
    })
}
/// Collect admitted numeric members using the selected origin-sensitive aggregation rules.
pub(super) fn coercev2_statistics_collection<P: EvaluationPort>(
    p: &mut P,
    args: &[P::Argument],
    policy: domain::Collection,
) -> Result<Vec<f64>, Failure<P::Operational>> {
    let mut values = Vec::new();
    for a in args {
        let len = p.collection_len(a)?;
        let max = values
            .len()
            .checked_add(len)
            .ok_or(Failure::UnsupportedCapability("collection size overflow"))?;
        p.check_collection(max).map_err(Failure::Operational)?;
        work(p, (len as u64).saturating_mul(3))?;
        values.reserve(len);
        let area = policy == domain::Collection::Mode && p.is_area(a)?;
        let scalar_channel = !p.is_reference(a)? && !p.is_array(a)?;
        for i in 0..len {
            let (s, o) = p.collection_cell(a, i)?;
            if policy == domain::Collection::Mode && s == Scalar::Missing {
                return Err(Failure::UnsupportedCapability("MODE MissingArgEval"));
            }
            let origin = if area {
                domain::Origin::Array
            } else if scalar_channel && o != Origin::Direct {
                domain::Origin::SelectedScalar
            } else {
                match o {
                    Origin::Direct => domain::Origin::Direct,
                    Origin::Reference => domain::Origin::Reference,
                    Origin::Array => domain::Origin::Array,
                }
            };
            let parse = policy == domain::Collection::PercentRank
                || ((o == Origin::Direct || scalar_channel)
                    && matches!(
                        policy,
                        domain::Collection::NumericAggregate
                            | domain::Collection::GeoMean
                            | domain::Collection::ExtremaA
                    ));
            let v = if parse {
                if let Scalar::Text(t) = s {
                    text_check(p, &t)?;
                    let n = match p.coercev2_text_number(&t) {
                        Ok(n) => Some(n),
                        Err(Failure::Spreadsheet(CellError::Value)) => None,
                        Err(other) => return Err(other),
                    };
                    domain::Value::Text { numeric: n }
                } else {
                    coercev2_statistics_collection_member(p, s, false)?
                }
            } else {
                coercev2_statistics_collection_member(p, s, false)?
            };
            // Domain single-cell collector may allocate one element; its allocation
            // and loop have already been checked/charged above. Errors stop traversal.
            let one = domain::coercev2_statistics_collection(
                policy,
                &[domain::Operand { value: v, origin }],
            )
            .map_err(Failure::Spreadsheet)?;
            values.extend(one);
        }
    }
    Ok(values)
}
fn coercev2_statistics_vector<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
    coerce: bool,
) -> Result<Vec<domain::Value>, Failure<P::Operational>> {
    let len = p.collection_len(a)?;
    p.check_collection(len).map_err(Failure::Operational)?;
    work(p, (len as u64).saturating_mul(3))?;
    let mut result = Vec::with_capacity(len);
    for i in 0..len {
        let (s, _) = p.collection_cell(a, i)?;
        if s == Scalar::Missing {
            return Err(Failure::UnsupportedCapability("paired MissingArgEval"));
        }
        if let Scalar::Error(e) = s {
            return Err(e.into());
        }
        result.push(coercev2_statistics_collection_member(p, s, coerce)?)
    }
    Ok(result)
}
/// Collect aligned operand pairs using the selected pinned pair coercion rules.
pub(super) fn coercev2_statistics_pairs<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
    b: &P::Argument,
    policy: domain::PairCollection,
) -> Result<domain::NumericPairs, Failure<P::Operational>> {
    // Source fully collects the first operand before the second, then validates
    // original lengths, and only then removes positional blanks/non-numeric pairs.
    let x = coercev2_statistics_vector(p, a, policy == domain::PairCollection::NumericCoercionV2)?;
    let y_len = p.collection_len(b)?;
    p.check_collection(x.len().saturating_add(y_len))
        .map_err(Failure::Operational)?;
    let y = coercev2_statistics_vector(p, b, policy == domain::PairCollection::NumericCoercionV2)?;
    p.check_collection(x.len().saturating_add(y.len()))
        .map_err(Failure::Operational)?;
    work(p, (x.len() as u64).saturating_mul(4))?;
    domain::coercev2_statistics_pairs(policy, &x, &y).map_err(Failure::Spreadsheet)
}
// XYNumericFunction and LinearRegressionFunction construct vector wrappers in
// argument order, checking direct ErrorEval immediately without dereferencing
// RefEval/TwoDEval. Their size check precedes all vector-member reads.
fn numeric_vector_len<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<usize, Failure<P::Operational>> {
    if !p.is_reference(a)?
        && !p.is_area(a)?
        && let Scalar::Error(error) = scalar(p, a)?
    {
        return Err(error.into());
    }
    p.collection_len(a)
}

/// Collect aligned numeric pairs under documented statistical or regression rules.
pub(super) fn numeric_only_pairs<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
    b: &P::Argument,
    regression: bool,
) -> Result<domain::NumericPairs, Failure<P::Operational>> {
    let a_len = numeric_vector_len(p, a)?;
    let b_len = numeric_vector_len(p, b)?;
    if a_len == 0 || a_len != b_len {
        return Err(CellError::NA.into());
    }
    p.check_collection(a_len.saturating_mul(2))
        .map_err(Failure::Operational)?;
    work(p, (a_len as u64).saturating_mul(6))?;
    let mut result = domain::NumericPairs {
        x: Vec::with_capacity(a_len),
        y: Vec::with_capacity(a_len),
        original_len: a_len,
    };
    let mut first_x_error = None;
    let mut first_y_error = None;
    for i in 0..a_len {
        // Regression's known_x is Excel arg1; XY's x is Excel arg0.
        let (x, _) = p.collection_cell(if regression { b } else { a }, i)?;
        let (y, _) = p.collection_cell(if regression { a } else { b }, i)?;
        if x == Scalar::Missing || y == Scalar::Missing {
            return Err(Failure::UnsupportedCapability("paired MissingArgEval"));
        }
        if let Scalar::Error(error) = x {
            if regression {
                return Err(error.into());
            }
            if first_x_error.is_none() {
                first_x_error = Some(error);
                continue;
            }
        }
        if let Scalar::Error(error) = y
            && first_y_error.is_none()
        {
            first_y_error = Some(error);
            continue;
        }
        let x = coercev2_statistics_collection_member(p, x, false)?;
        let y = coercev2_statistics_collection_member(p, y, false)?;
        if let Some((x, y)) = poirot_coercionv2::statistics::coercev2_statistics_numeric_pair(x, y)
        {
            // Store in original argument order for the pure algorithm boundary.
            result.x.push(if regression { y } else { x });
            result.y.push(if regression { x } else { y });
        }
    }
    if let Some(error) = first_x_error.or(first_y_error) {
        return Err(error.into());
    }
    Ok(result)
}
