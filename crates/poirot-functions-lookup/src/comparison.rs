use crate::operands::charge;
use crate::wildcard::wildcard_matches;
use crate::{Failure, OperationalFailure, Scalar, Work, Workbook};
use std::cmp::Ordering;
/// Compare scalar lookup keys under the selected type and text ordering rules.
pub(super) fn compare(
    key: &Scalar,
    item: &Scalar,
    tolerant: bool,
    wildcard: bool,
    w: &mut impl Workbook,
) -> Result<Option<Ordering>, Failure> {
    charge(w, Work::SearchSteps, 1)?;
    compare_inner(key, item, tolerant, wildcard, w)
}
fn compare_inner(
    key: &Scalar,
    item: &Scalar,
    tolerant: bool,
    wildcard: bool,
    w: &mut impl Workbook,
) -> Result<Option<Ordering>, Failure> {
    Ok(match (key, item) {
        (Scalar::Number(a), Scalar::Number(b)) if !wildcard => a.partial_cmp(b),
        (Scalar::Bool(a), Scalar::Bool(b)) => Some(a.cmp(b)),
        (Scalar::Text(a), Scalar::Text(b)) => {
            if wildcard && has_wildcards(a) {
                Some(if wildcard_matches(a, b, w)? {
                    Ordering::Equal
                } else {
                    Ordering::Less
                })
            } else {
                {
                    charge(
                        w,
                        Work::SearchText,
                        a.len()
                            .checked_add(b.len())
                            .ok_or(Failure::Operational(OperationalFailure::ResourceOverflow))?,
                    )?;
                    Some(poirot_text::fold::compare(a, b))
                }
            }
        }
        (Scalar::Blank, _) if tolerant => {
            compare(&coercev2_lookup_blank_key(true), item, true, wildcard, w)?
        }
        (Scalar::Blank, _) => compare(&coercev2_lookup_blank_key(false), item, false, false, w)?,
        // TolerantStringLookupComparer retains the StringEval class check on candidates.
        (Scalar::Number(_), Scalar::Text(b)) if tolerant && wildcard => {
            let Scalar::Number(number) = key else {
                return Ok(None);
            };
            charge(w, Work::SearchText, 32)?;
            let a = w.coercev2_number_to_text(*number)?;
            {
                charge(
                    w,
                    Work::SearchText,
                    a.len()
                        .checked_add(b.len())
                        .ok_or(Failure::Operational(OperationalFailure::ResourceOverflow))?,
                )?;
                Some(poirot_text::fold::compare(&a, b))
            }
        }
        _ => None,
    })
}
fn has_wildcards(pattern: &str) -> bool {
    pattern.contains(['*', '?', '~'])
}

fn coercev2_lookup_blank_key(tolerant: bool) -> Scalar {
    poirot_coercionv2::lookup::coercev2_lookup_blank_key(tolerant)
}
