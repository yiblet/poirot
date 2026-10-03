//! Preserves the earlier MATCH and lookup binary-search selection algorithm.
//! Reads stay in a callback so repeated comparisons retain their read costs and failures.
use super::*;

/// Pinned Match.findIndexOfValue and LookupUtils.performBinarySearch policies.
pub(super) fn select_index(
    mut len: usize,
    mode: SearchMode,
    lookup: bool,
    mut read: impl FnMut(usize) -> FunctionResult<Option<Ordering>>,
) -> FunctionResult<Option<usize>> {
    if len == 0 {
        return Ok(None);
    }
    if mode == SearchMode::Exact {
        for i in 0..len {
            if read(i)? == Some(Ordering::Equal) {
                return Ok(Some(i));
            }
        }
        return Ok(None);
    }
    if !lookup {
        if mode == SearchMode::Ascending {
            for i in (0..len).rev() {
                if matches!(read(i)?, Some(Ordering::Equal | Ordering::Greater)) {
                    return Ok(Some(i));
                }
            }
            return Ok(None);
        }
        for i in 0..len {
            match read(i)? {
                Some(Ordering::Equal) => return Ok(Some(i)),
                Some(Ordering::Greater) => return Ok(i.checked_sub(1)),
                _ => {}
            }
        }
        return Ok(Some(len - 1));
    }
    let mut low: isize = -1;
    loop {
        if len as isize - low < 2 {
            return Ok(usize::try_from(low).ok());
        }
        let mut mid = (low + (len as isize - low) / 2) as usize;
        let mut cmp = read(mid)?;
        if cmp.is_none() {
            let original = mid;
            loop {
                mid += 1;
                if mid == len {
                    len = original;
                    break;
                }
                cmp = read(mid)?;
                if cmp == Some(Ordering::Less) && mid == len - 1 {
                    len = original;
                    break;
                }
                if cmp.is_none() {
                    continue;
                }
                if cmp == Some(Ordering::Equal) {
                    break;
                }
                if cmp == Some(Ordering::Less) {
                    len = mid;
                } else {
                    low = mid as isize;
                }
                break;
            }
            if cmp != Some(Ordering::Equal) {
                continue;
            }
            // Source re-compares the matched candidate after mismatch stepping.
            cmp = read(mid)?;
        }
        if cmp == Some(Ordering::Equal) {
            for i in mid + 1..len {
                if read(i)? != Some(Ordering::Equal) {
                    return Ok(Some(i - 1));
                }
            }
            return Ok(Some(len - 1));
        }
        if cmp == Some(Ordering::Less) {
            len = mid;
        } else {
            low = mid as isize;
        }
    }
}
