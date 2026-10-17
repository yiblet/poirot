use crate::operands::charge;
use crate::wildcard::wildcard_matches;
use crate::{Failure, OperationalFailure, Scalar, Unsupported, Work, Workbook};
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
    if let Scalar::Text(text) = key {
        if !text.is_ascii() {
            return Err(Failure::Unsupported(Unsupported::UnicodeSearch));
        }
    }
    if let Scalar::Text(text) = item {
        if !text.is_ascii() {
            return Err(Failure::Unsupported(Unsupported::UnicodeSearch));
        }
    }
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
                    Some(a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()))
                }
            }
        }
        (Scalar::Blank, _) if tolerant => {
            // TODO(coercion-non-argument): [MEDIUM] Search comparison replaces a blank key with text or zero; incorrect mode selection can match a different key.
            compare(&coerce_lookup_blank_key(true), item, true, wildcard, w)?
        }
        // TODO(coercion-non-argument): [MEDIUM] Search comparison replaces a blank key with text or zero; incorrect mode selection can match a different key.
        (Scalar::Blank, _) => compare(&coerce_lookup_blank_key(false), item, false, false, w)?,
        // TolerantStringLookupComparer retains the StringEval class check on candidates.
        (Scalar::Number(_), Scalar::Text(b)) if tolerant && wildcard => {
            let Scalar::Number(number) = key else {
                return Ok(None);
            };
            charge(w, Work::SearchText, 32)?;
            // TODO(coercion-non-argument): [LOW] Wildcard comparison converts a numeric key to text; the converted key must not escape the search comparison.
            let a = w.coerce_number_to_text(*number)?;
            {
                charge(
                    w,
                    Work::SearchText,
                    a.len()
                        .checked_add(b.len())
                        .ok_or(Failure::Operational(OperationalFailure::ResourceOverflow))?,
                )?;
                Some(a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()))
            }
        }
        _ => None,
    })
}
fn has_wildcards(pattern: &str) -> bool {
    pattern.contains(['*', '?', '~'])
}

fn coerce_lookup_blank_key(tolerant: bool) -> Scalar {
    if tolerant {
        Scalar::Text(String::new())
    } else {
        Scalar::Number(0.0)
    }
}
