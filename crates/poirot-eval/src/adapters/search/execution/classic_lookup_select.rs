//! Classic lookup select concern; indexed callbacks retain their host effects.
use super::*;

/// Pinned Match.findIndexOfValue and LookupUtils.performBinarySearch policies.
pub(super) trait CandidateReader {
    /// Find the next candidate in a half-open logical interval.
    fn next(&mut self, first: usize, end: usize) -> FunctionResult<Option<usize>>;
    /// Find the last candidate before an exclusive logical endpoint.
    fn previous(&mut self, end: usize) -> FunctionResult<Option<usize>>;
    /// Compare the key with one candidate, or report a noncomparable value.
    fn compare(&mut self, index: usize) -> FunctionResult<Option<Ordering>>;
}
#[cfg(test)]
/// Exercise the selection policy over a dense indexed test callback.
pub(super) fn select_index(
    len: usize,
    mode: SearchMode,
    lookup: bool,
    read: impl FnMut(usize) -> FunctionResult<Option<Ordering>>,
) -> FunctionResult<Option<usize>> {
    /// Dense candidate reader adapting a test comparison callback.
    struct Dense<F>(F);
    impl<F: FnMut(usize) -> FunctionResult<Option<Ordering>>> CandidateReader for Dense<F> {
        fn next(&mut self, first: usize, end: usize) -> FunctionResult<Option<usize>> {
            Ok((first < end).then_some(first))
        }
        fn previous(&mut self, end: usize) -> FunctionResult<Option<usize>> {
            Ok(end.checked_sub(1))
        }
        fn compare(&mut self, index: usize) -> FunctionResult<Option<Ordering>> {
            self.0(index)
        }
    }
    select_with_reader(len, mode, lookup, &mut Dense(read))
}
/// Select a logical candidate using pinned MATCH or lookup search rules.
pub(super) fn select_with_reader(
    mut len: usize,
    mode: SearchMode,
    lookup: bool,
    reader: &mut impl CandidateReader,
) -> FunctionResult<Option<usize>> {
    if len == 0 {
        return Ok(None);
    }
    if mode == SearchMode::Exact {
        let mut next = reader.next(0, len)?;
        while let Some(i) = next {
            if reader.compare(i)? == Some(Ordering::Equal) {
                return Ok(Some(i));
            }
            next = reader.next(i + 1, len)?;
        }
        return Ok(None);
    }
    if !lookup {
        if mode == SearchMode::Ascending {
            let mut previous = reader.previous(len)?;
            while let Some(i) = previous {
                if matches!(
                    reader.compare(i)?,
                    Some(Ordering::Equal | Ordering::Greater)
                ) {
                    return Ok(Some(i));
                }
                previous = reader.previous(i)?;
            }
            return Ok(None);
        }
        let mut next = reader.next(0, len)?;
        while let Some(i) = next {
            match reader.compare(i)? {
                Some(Ordering::Equal) => return Ok(Some(i)),
                Some(Ordering::Greater) => return Ok(i.checked_sub(1)),
                _ => {}
            }
            next = reader.next(i + 1, len)?;
        }
        return Ok(Some(len - 1));
    }
    let mut low: isize = -1;
    loop {
        if len as isize - low < 2 {
            return Ok(usize::try_from(low).ok());
        }
        let mut mid = (low + (len as isize - low) / 2) as usize;
        let mut cmp = if reader.next(mid, mid + 1)? == Some(mid) {
            reader.compare(mid)?
        } else {
            None
        };
        if cmp.is_none() {
            let original = mid;
            loop {
                let Some(next) = reader.next(mid + 1, len)? else {
                    len = original;
                    break;
                };
                mid = next;
                cmp = reader.compare(mid)?;
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
            cmp = reader.compare(mid)?;
        }
        if cmp == Some(Ordering::Equal) {
            for i in mid + 1..len {
                if reader.next(i, i + 1)? != Some(i) || reader.compare(i)? != Some(Ordering::Equal)
                {
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
