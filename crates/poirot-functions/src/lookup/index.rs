use crate::lookup::comparison::compare;
use crate::lookup::operands::charge;
use crate::lookup::{LookupError, LookupValueError, MatchMode, Scalar, SearchMode, Work, Workbook};
use std::cmp::Ordering;
// Mirror handleMidValueTypeMismatch's separate bounds and comparison flags.
// The workbook port keeps each repeated comparison fallible and budgeted.
#[allow(clippy::too_many_arguments)]
fn mismatch_step(
    key: &Scalar,
    vector: &[Scalar],
    low: &mut isize,
    high: &mut isize,
    mid: isize,
    tolerant: bool,
    wild: bool,
    reverse: bool,
    w: &mut impl Workbook,
) -> Result<Option<usize>, LookupError> {
    let mut next = mid + 1;
    while next < *high {
        let cmp = compare(
            key,
            vector.get(next as usize).ok_or(LookupValueError::Value)?,
            tolerant,
            wild,
            w,
        )?;
        if (cmp == Some(Ordering::Less) && !reverse || cmp == Some(Ordering::Greater) && reverse)
            && next == *high - 1
        {
            *high = mid;
            return Ok(None);
        }
        if let Some(cmp) = cmp {
            if cmp == Ordering::Equal {
                return Ok(Some(next as usize));
            }
            if if reverse {
                cmp == Ordering::Greater
            } else {
                cmp == Ordering::Less
            } {
                *high = next;
            } else {
                *low = next;
            }
            return Ok(None);
        }
        next += 1;
    }
    *high = mid;
    Ok(None)
}
/// Select an approximate-match candidate in an admitted search vector.
pub fn approximate_index(
    key: &Scalar,
    vector: &[Scalar],
    w: &mut impl Workbook,
) -> Result<usize, LookupError> {
    if let Scalar::Error(e) = key {
        return Err((*e).into());
    }
    let (mut low, mut high) = (-1isize, vector.len() as isize);
    while high - low >= 2 {
        let mid = low + (high - low) / 2;
        match compare(
            key,
            vector.get(mid as usize).ok_or(LookupValueError::Value)?,
            false,
            false,
            w,
        )? {
            None => {
                if let Some(index) = mismatch_step(
                    key, vector, &mut low, &mut high, mid, false, false, false, w,
                )? {
                    return Ok(index);
                }
            }
            Some(Ordering::Equal) => {
                let mut index = mid as usize;
                while index + 1 < high as usize
                    && compare(
                        key,
                        vector.get(index + 1).ok_or(LookupValueError::Value)?,
                        false,
                        false,
                        w,
                    )? == Some(Ordering::Equal)
                {
                    index += 1;
                }
                return Ok(index);
            }
            Some(Ordering::Less) => high = mid,
            Some(Ordering::Greater) => low = mid,
        }
    }
    if low < 0 {
        Err(LookupValueError::Na.into())
    } else {
        Ok(low as usize)
    }
}
/// Select a position using XMATCH match and traversal modes.
pub fn xmatch_index(
    key: &Scalar,
    vector: &[Scalar],
    mode: MatchMode,
    direction: SearchMode,
    w: &mut impl Workbook,
) -> Result<usize, LookupError> {
    if let Scalar::Error(e) = key {
        return Err((*e).into());
    }
    let modified;
    let key = if matches!(mode, MatchMode::Smaller | MatchMode::Larger) {
        if let Scalar::Text(s) = key {
            charge(w, Work::SearchText, s.len())?;
            modified = Scalar::Text(
                s.chars()
                    .take_while(|c| !matches!(c, '~' | '?' | '*'))
                    .collect(),
            );
            &modified
        } else {
            key
        }
    } else {
        key
    };
    // POI enables wildcard matching for text in all X modes through isMatchFunction=true.
    let wild = matches!(key, Scalar::Text(_)) || mode == MatchMode::Wildcard;
    let mut best: Option<usize> = None;
    if matches!(direction, SearchMode::Forward | SearchMode::Backward) {
        for step in 0..vector.len() {
            let index = if direction == SearchMode::Backward {
                vector.len() - step - 1
            } else {
                step
            };
            if let Some(cmp) = compare(
                key,
                vector.get(index).ok_or(LookupValueError::Value)?,
                true,
                wild,
                w,
            )? {
                if cmp == Ordering::Equal {
                    return Ok(index);
                }
                update_best(&mut best, index, cmp, mode, vector, w)?;
            }
        }
    } else {
        let reverse = direction == SearchMode::BinaryDescending;
        let (mut low, mut high) = (-1isize, vector.len() as isize);
        let mut searched = std::collections::HashSet::new();
        while high - low >= 2 {
            let mid = low + (high - low) / 2;
            if !searched.insert(mid) {
                break;
            }
            match compare(
                key,
                vector.get(mid as usize).ok_or(LookupValueError::Value)?,
                true,
                wild,
                w,
            )? {
                None => {
                    if let Some(index) = mismatch_step(
                        key, vector, &mut low, &mut high, mid, true, wild, reverse, w,
                    )? {
                        return Ok(index);
                    }
                }
                Some(cmp) => {
                    if cmp == Ordering::Equal {
                        return Ok(mid as usize);
                    }
                    update_best(&mut best, mid as usize, cmp, mode, vector, w)?;
                    if if reverse {
                        cmp == Ordering::Greater
                    } else {
                        cmp == Ordering::Less
                    } {
                        high = mid;
                    } else {
                        low = mid;
                    }
                }
            }
        }
    }
    best.ok_or(LookupError::Spreadsheet(LookupValueError::Na))
}

fn update_best(
    best: &mut Option<usize>,
    index: usize,
    cmp: Ordering,
    mode: MatchMode,
    vector: &[Scalar],
    w: &mut impl Workbook,
) -> Result<(), LookupError> {
    let eligible = mode == MatchMode::Smaller && cmp == Ordering::Greater
        || mode == MatchMode::Larger && cmp == Ordering::Less;
    if eligible {
        let improves = if let Some(old) = *best {
            compare(
                vector.get(index).ok_or(LookupValueError::Value)?,
                vector.get(old).ok_or(LookupValueError::Value)?,
                true,
                matches!(vector.get(index), Some(Scalar::Text(_))),
                w,
            )?
            .is_some_and(|c| {
                if mode == MatchMode::Smaller {
                    c == Ordering::Greater
                } else {
                    c == Ordering::Less
                }
            })
        } else {
            true
        };
        if improves {
            *best = Some(index);
        }
    }
    Ok(())
}
