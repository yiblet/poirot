//! Typed classic MATCH traversal, separate from pinned POI and XMATCH search.
use std::cmp::Ordering;

/// Approximate MATCH contract; exact matching retains its existing first-match path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassicMatchMode {
    /// Bisect an ascending vector, retaining original logical positions.
    Ascending,
    /// Scan a descending vector until the first exact match or lower candidate.
    Descending,
}

/// Fallible host effects for typed MATCH candidate selection.
/// `None` comparisons mean a different scalar type, blank, or spreadsheet error;
/// operational failures remain `Err` and must never be skipped.
pub trait MatchCandidateReader {
    /// Host failure type, preserved unchanged by selection.
    type Error;
    /// Find the next populated logical position in a half-open interval.
    fn next(&mut self, first: usize, end: usize) -> Result<Option<usize>, Self::Error>;
    /// Find the previous populated logical position before an exclusive endpoint.
    fn previous(&mut self, end: usize) -> Result<Option<usize>, Self::Error>;
    /// Compare the lookup key to a candidate, without converting candidate types.
    fn compare(&mut self, index: usize) -> Result<Option<Ordering>, Self::Error>;
}

/// A malformed host position is distinct from a failure while reading a candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchSelectionFailure<E> {
    /// Preserve the host's original operational failure.
    Reader(E),
    /// A reader returned a position outside the requested logical interval.
    InvalidPosition,
}
fn next<R: MatchCandidateReader>(
    reader: &mut R,
    first: usize,
    end: usize,
) -> Result<Option<usize>, MatchSelectionFailure<R::Error>> {
    let index = reader
        .next(first, end)
        .map_err(MatchSelectionFailure::Reader)?;
    if index.is_some_and(|index| index < first || index >= end) {
        return Err(MatchSelectionFailure::InvalidPosition);
    }
    Ok(index)
}
fn previous<R: MatchCandidateReader>(
    reader: &mut R,
    end: usize,
) -> Result<Option<usize>, MatchSelectionFailure<R::Error>> {
    let index = reader
        .previous(end)
        .map_err(MatchSelectionFailure::Reader)?;
    if index.is_some_and(|index| index >= end) {
        return Err(MatchSelectionFailure::InvalidPosition);
    }
    Ok(index)
}
/// Select approximate MATCH under the typed ClosedXML/Excel-documentation contract.
/// Unordered vectors follow the same traversal; no global nearest-value search is made.
pub fn classic_match_index<R: MatchCandidateReader>(
    len: usize,
    mode: ClassicMatchMode,
    reader: &mut R,
) -> Result<Option<usize>, MatchSelectionFailure<R::Error>> {
    if len == 0 {
        return Ok(None);
    }
    if mode == ClassicMatchMode::Descending {
        let mut candidate = None;
        let mut position = next(reader, 0, len)?;
        while let Some(index) = position {
            match reader
                .compare(index)
                .map_err(MatchSelectionFailure::Reader)?
            {
                Some(Ordering::Equal) => return Ok(Some(index)),
                Some(Ordering::Greater) => return Ok(candidate),
                Some(Ordering::Less) => candidate = Some(index),
                None => {}
            }
            position = next(reader, index.saturating_add(1), len)?;
        }
        return Ok(candidate);
    }

    // Inclusive bounds matter for the bisection trajectory on unordered vectors.
    // SAFETY: The function returns above when len is zero.
    let (mut low, mut high) = (0, len - 1);
    while low < high {
        // SAFETY: low < high in this branch, and the midpoint is at most high.
        let middle = low + (high - low) / 2;
        let mut position = next(reader, middle, high.saturating_add(1))?;
        let mut compared = None;
        while let Some(index) = position {
            if let Some(order) = reader
                .compare(index)
                .map_err(MatchSelectionFailure::Reader)?
            {
                compared = Some((index, order));
                break;
            }
            position = next(reader, index.saturating_add(1), high.saturating_add(1))?;
        }
        match compared {
            Some((index, Ordering::Equal)) => {
                let mut last = index;
                // Exact duplicate runs stop at a blank or a different scalar type.
                while last.saturating_add(1) < len
                    && next(reader, last.saturating_add(1), last.saturating_add(2))?
                        == Some(last.saturating_add(1))
                    && reader
                        .compare(last.saturating_add(1))
                        .map_err(MatchSelectionFailure::Reader)?
                        == Some(Ordering::Equal)
                {
                    last = last.saturating_add(1);
                }
                return Ok(Some(last));
            }
            Some((index, Ordering::Greater)) => low = index.saturating_add(1).min(high),
            Some((index, Ordering::Less)) => high = index.saturating_sub(1).max(low),
            None => high = middle.saturating_sub(1).max(low),
        }
    }
    // The final bound can be above the key. Retreat through original positions,
    // skipping noncomparable values rather than returning a neighboring type.
    let mut position = previous(reader, low.saturating_add(1))?;
    while let Some(index) = position {
        if matches!(
            reader
                .compare(index)
                .map_err(MatchSelectionFailure::Reader)?,
            Some(Ordering::Equal | Ordering::Greater)
        ) {
            return Ok(Some(index));
        }
        position = previous(reader, index)?;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Reader {
        values: Vec<Option<i32>>,
        key: i32,
        reads: Vec<usize>,
        fail: Option<usize>,
    }
    impl MatchCandidateReader for Reader {
        type Error = &'static str;
        fn next(&mut self, first: usize, end: usize) -> Result<Option<usize>, Self::Error> {
            Ok((first < end).then_some(first))
        }
        fn previous(&mut self, end: usize) -> Result<Option<usize>, Self::Error> {
            Ok(end.checked_sub(1))
        }
        fn compare(&mut self, index: usize) -> Result<Option<Ordering>, Self::Error> {
            self.reads.push(index);
            if self.fail == Some(index) {
                return Err("budget refused");
            }
            Ok(self.values[index].map(|v| self.key.cmp(&v)))
        }
    }
    fn reader(values: &[Option<i32>], key: i32) -> Reader {
        Reader {
            values: values.to_vec(),
            key,
            reads: vec![],
            fail: None,
        }
    }
    #[test]
    fn sorted_lengths_duplicates_and_both_modes_follow_documented_bounds() {
        // Expected positions follow the documented sorted-vector contract,
        // independently of the bisection implementation.
        for len in 0..65 {
            let ascending: Vec<_> = (0..len).map(|i| Some(i / 3)).collect();
            let descending: Vec<_> = ascending.iter().copied().rev().collect();
            for key in -1..24 {
                let expected = ascending.iter().rposition(|v| v.is_some_and(|v| v <= key));
                assert_eq!(
                    classic_match_index(
                        len as usize,
                        ClassicMatchMode::Ascending,
                        &mut reader(&ascending, key)
                    ),
                    Ok(expected)
                );
                let expected = descending
                    .iter()
                    .position(|v| *v == Some(key))
                    .or_else(|| descending.iter().rposition(|v| v.is_some_and(|v| v > key)));
                assert_eq!(
                    classic_match_index(
                        len as usize,
                        ClassicMatchMode::Descending,
                        &mut reader(&descending, key)
                    ),
                    Ok(expected)
                );
            }
        }
    }
    #[test]
    fn candidate_identity_and_operational_read_order_survive_type_mismatches() {
        let values = [Some(10), None, None, Some(5), Some(3)];
        let mut r = reader(&values, 6);
        assert_eq!(
            classic_match_index(values.len(), ClassicMatchMode::Descending, &mut r),
            Ok(Some(0))
        );
        assert_eq!(r.reads, [0, 1, 2, 3]);
        r.reads.clear();
        r.fail = Some(2);
        assert_eq!(
            classic_match_index(values.len(), ClassicMatchMode::Descending, &mut r),
            Err(MatchSelectionFailure::Reader("budget refused"))
        );
        assert_eq!(r.reads, [0, 1, 2]);
        let mut r = reader(
            &[
                Some(5),
                Some(15),
                Some(18),
                Some(18),
                Some(11),
                Some(1),
                Some(15),
                Some(17),
            ],
            12,
        );
        assert_eq!(
            classic_match_index(8, ClassicMatchMode::Ascending, &mut r),
            Ok(Some(0))
        );
        assert_eq!(r.reads, [3, 1, 0]);
        r.reads.clear();
        r.fail = Some(1);
        assert_eq!(
            classic_match_index(8, ClassicMatchMode::Ascending, &mut r),
            Err(MatchSelectionFailure::Reader("budget refused"))
        );
        assert_eq!(r.reads, [3, 1]);
    }
    #[test]
    fn malformed_readers_fail_before_arithmetic_or_nonprogressing_loops() {
        struct Host {
            invalid_next: bool,
            invalid_previous: bool,
            order: Ordering,
        }
        impl MatchCandidateReader for Host {
            type Error = ();
            fn next(&mut self, first: usize, end: usize) -> Result<Option<usize>, ()> {
                if self.invalid_next {
                    return Ok(Some(0));
                }
                Ok((first < end).then(|| end - 1))
            }
            fn previous(&mut self, end: usize) -> Result<Option<usize>, ()> {
                Ok(if self.invalid_previous {
                    Some(end)
                } else {
                    end.checked_sub(1)
                })
            }
            fn compare(&mut self, _: usize) -> Result<Option<Ordering>, ()> {
                Ok(Some(self.order))
            }
        }
        let mut host = Host {
            invalid_next: false,
            invalid_previous: false,
            order: Ordering::Greater,
        };
        assert_eq!(
            classic_match_index(usize::MAX, ClassicMatchMode::Ascending, &mut host),
            Ok(Some(usize::MAX - 1))
        );
        host.order = Ordering::Less;
        assert_eq!(
            classic_match_index(usize::MAX, ClassicMatchMode::Descending, &mut host),
            Ok(Some(usize::MAX - 1))
        );
        host.invalid_next = true;
        assert_eq!(
            classic_match_index(2, ClassicMatchMode::Descending, &mut host),
            Err(MatchSelectionFailure::InvalidPosition)
        );
        host.invalid_next = false;
        host.invalid_previous = true;
        assert_eq!(
            classic_match_index(1, ClassicMatchMode::Ascending, &mut host),
            Err(MatchSelectionFailure::InvalidPosition)
        );
        assert_eq!(
            classic_match_index(0, ClassicMatchMode::Ascending, &mut host),
            Ok(None)
        );
    }
}
