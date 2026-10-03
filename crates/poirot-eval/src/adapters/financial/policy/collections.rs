//! Lazy, origin-sensitive cash-flow collection.
use super::coercion::{parse_operand_number, text_check, work};
use super::{CellError, EvaluationPort, Failure, Origin, Scalar};

/// Collect financial cash flows lazily with origin-sensitive coercion and admission.
pub(super) fn collect<P: EvaluationPort>(
    p: &mut P,
    args: &[P::Argument],
) -> Result<Vec<f64>, Failure<P::Operational>> {
    let mut values = Vec::new();
    for a in args {
        let len = p.collection_len(a)?;
        let limit = values
            .len()
            .checked_add(len)
            .ok_or(Failure::UnsupportedCapability("collection size overflow"))?;
        p.check_collection(limit).map_err(Failure::Operational)?;
        work(p, len as u64)?;
        values.reserve(len);
        for i in 0..len {
            let (s, o) = p.collection_cell(a, i)?;
            let n = match s {
                Scalar::Number(n) => Some(n),
                Scalar::Error(e) => return Err(e.into()),
                Scalar::Boolean(b) if o == Origin::Direct => Some(u8::from(b) as f64),
                Scalar::Text(t) if o == Origin::Direct => {
                    text_check(p, &t)?;
                    Some(parse_operand_number(&t).ok_or(CellError::Value)?)
                }
                _ => None,
            };
            if let Some(n) = n {
                if !n.is_finite() {
                    return Err(CellError::Num.into());
                }
                values.push(n)
            }
        }
    }
    Ok(values)
}
