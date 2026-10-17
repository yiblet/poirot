//! Lazy, origin-sensitive cash-flow collection.
use super::coercionv2::{coercev2_financial_decimal_text, text_check, work};
use super::{CellError, EvaluationPort, Failure};

/// Collect financial cash flows lazily with origin-sensitive coercion and admission.
pub(super) fn coercev2_financial_cashflows<P: EvaluationPort>(
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
            let n = poirot_coercionv2::collections::coercev2_cashflow_member(
                (&s).into(),
                o,
                |input| match input {
                    poirot_coercionv2::compatibility::DeferredNumber::Text(
                        poirot_coercionv2::TextRef::Utf8(text),
                    ) => {
                        text_check(p, text)?;
                        coercev2_financial_decimal_text(text)
                            .ok_or(Failure::Spreadsheet(CellError::Value))
                    }
                    _ => Err(Failure::Spreadsheet(CellError::Value)),
                },
            )?;
            if let Some(n) = n {
                values.push(n)
            }
        }
    }
    Ok(values)
}
